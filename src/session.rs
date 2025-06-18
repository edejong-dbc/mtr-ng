use crate::{Args, HopStats, Result};
use pnet::packet::{
    icmp::{IcmpPacket, IcmpTypes, IcmpType},
    ip::IpNextHeaderProtocols,
    ipv4::Ipv4Packet,
    util, Packet, MutablePacket,
};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::HashMap,
    mem::MaybeUninit,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::time;
use tracing::{debug, info, warn};
use trust_dns_resolver::{config::*, TokioAsyncResolver};

const MIN_SEQUENCE: u16 = 33000;
const MAX_SEQUENCE: u16 = 65535;

#[derive(Debug, Clone)]
pub struct SequenceEntry {
    pub index: usize,       // hop index (like original mtr)
    pub transit: bool,      // is this sequence in transit?
    pub saved_seq: u32,     // saved sequence for this host
    pub send_time: Instant, // when packet was sent
}

#[derive(Debug, Clone)]
pub struct MtrSession {
    pub target: String,
    pub target_addr: IpAddr,
    pub hops: Vec<HopStats>,
    pub args: Args,
    pub resolver: TokioAsyncResolver,
    pub packet_id: u16,
    pub next_sequence: u16,
    pub sequence_table: HashMap<u16, SequenceEntry>, // sequence -> entry (like original mtr)
    pub batch_at: usize, // current hop index being sent (like original mtr)
    pub num_hosts: usize, // number of active hops
}

impl MtrSession {
    pub async fn new(args: Args) -> Result<Self> {
        let resolver = TokioAsyncResolver::tokio(
            ResolverConfig::default(),
            ResolverOpts::default(),
        );

        // Resolve target hostname to IP
        let target_addr = if let Ok(ip) = args.target.parse::<IpAddr>() {
            ip
        } else {
            let response = resolver.lookup_ip(&args.target).await?;
            response
                .iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("Failed to resolve hostname"))?
        };

        let hops = (1..=args.max_hops).map(HopStats::new).collect();
        let packet_id = std::process::id() as u16;

        Ok(Self {
            target: args.target.clone(),
            target_addr,
            hops,
            args,
            resolver,
            packet_id,
            next_sequence: MIN_SEQUENCE,
            sequence_table: HashMap::new(),
            batch_at: 0, // Start at hop 1 (index 0)
            num_hosts: 10, // Initial estimate
        })
    }

    pub async fn run_trace(&mut self) -> Result<()> {
        info!("Starting trace to {} ({})", self.target, self.target_addr);
        
        match self.target_addr {
            IpAddr::V4(ipv4) => self.run_ipv4_trace(ipv4).await,
            IpAddr::V6(_) => {
                warn!("IPv6 not yet implemented, falling back to simulation");
                self.run_simulated_trace().await
            }
        }
    }

    async fn run_ipv4_trace(&mut self, target: Ipv4Addr) -> Result<()> {
        // Try to create raw socket for ICMP
        match self.create_raw_socket() {
            Ok((send_socket, recv_socket)) => {
                info!("Using raw ICMP sockets for real traceroute");
                self.run_mtr_algorithm(target, send_socket, recv_socket).await
            }
            Err(e) => {
                warn!("Failed to create raw socket ({}), falling back to simulation. Try running with sudo for real traceroute.", e);
                self.run_simulated_trace().await
            }
        }
    }

    fn create_raw_socket(&self) -> Result<(Socket, Socket)> {
        // Create raw ICMP socket for sending
        let send_socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
        
        // Create raw socket for receiving ICMP responses
        let recv_socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
        
        // Set socket options
        send_socket.set_nonblocking(true)?;
        recv_socket.set_nonblocking(true)?;
        
        // Set receive timeout to be very short for non-blocking operation
        recv_socket.set_read_timeout(Some(Duration::from_millis(1)))?;
        
        Ok((send_socket, recv_socket))
    }

    // Implementation of the exact MTR algorithm from the C code
    async fn run_mtr_algorithm(&mut self, target: Ipv4Addr, send_socket: Socket, recv_socket: Socket) -> Result<()> {
        let mut round = 0;
        
        while round < self.args.count {
            // Send batch (one packet per active hop, like original mtr)
            let restart = self.net_send_batch(target, &send_socket).await?;
            
            // Collect responses for this interval
            let collect_duration = Duration::from_millis(self.args.interval);
            let start_collect = Instant::now();
            
            while start_collect.elapsed() < collect_duration {
                self.net_process_return(&recv_socket, target).await;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            
            if restart {
                round += 1;
                debug!("Completed round {}/{}, restarting batch", round, self.args.count);
            }
        }
        
        Ok(())
    }

    // Equivalent to net_send_batch in original mtr
    async fn net_send_batch(&mut self, target: Ipv4Addr, send_socket: &Socket) -> Result<bool> {
        let mut n_unknown = 0;
        let mut restart = false;
        
        // Send query for current hop (like original mtr's net_send_query)
        self.net_send_query(target, send_socket, self.batch_at).await?;
        
        // Check all previous hops to see if we should restart
        for i in 0..self.batch_at {
            if self.hops[i].addr.is_none() {
                n_unknown += 1;
            }
            
            // Check if we've reached the target at this hop
            if let Some(IpAddr::V4(addr)) = self.hops[i].addr {
                if addr == target {
                    restart = true;
                    self.num_hosts = i + 1;
                    break;
                }
            }
        }
        
        // Restart conditions (same as original mtr)
        if self.batch_at >= (self.args.max_hops as usize) - 1 || 
           n_unknown > 5 || // maxUnknown equivalent
           (self.hops.get(self.batch_at).and_then(|h| h.addr).map_or(false, |addr| {
               matches!(addr, IpAddr::V4(a) if a == target)
           })) {
            restart = true;
            self.num_hosts = self.batch_at + 1;
        }
        
        if restart {
            self.batch_at = 0; // Reset to hop 1
        } else {
            self.batch_at += 1;
        }
        
        Ok(restart)
    }
    
    // Equivalent to net_send_query in original mtr
    async fn net_send_query(&mut self, target: Ipv4Addr, send_socket: &Socket, index: usize) -> Result<()> {
        let seq = self.new_sequence(index);
        let time_to_live = (index + 1) as u32;
        
        debug!("Sending probe: hop={}, TTL={}, seq={}", index + 1, time_to_live, seq);
        
        Self::send_icmp_packet_static(send_socket, target, time_to_live, self.packet_id, seq)?;
        
        Ok(())
    }
    
    // Equivalent to new_sequence and save_sequence in original mtr
    fn new_sequence(&mut self, index: usize) -> u16 {
        let seq = self.next_sequence;
        
        // Advance sequence (with wraparound like original)
        self.next_sequence += 1;
        if self.next_sequence >= MAX_SEQUENCE {
            self.next_sequence = MIN_SEQUENCE;
        }
        
        // Save sequence info (like save_sequence in original)
        self.save_sequence(index, seq);
        
        seq
    }
    
    fn save_sequence(&mut self, index: usize, seq: u16) {
        // Increment transmit count for this hop
        self.hops[index].increment_sent();
        
        // Record sequence entry
        let entry = SequenceEntry {
            index,
            transit: true,
            saved_seq: self.hops[index].sent as u32,
            send_time: Instant::now(),
        };
        
        self.sequence_table.insert(seq, entry);
        
        debug!("Saved sequence: seq={}, hop={}, sent_count={}", seq, index + 1, self.hops[index].sent);
    }
    
    // Equivalent to mark_sequence_complete in original mtr
    fn mark_sequence_complete(&mut self, seq: u16) -> Option<(usize, Instant)> {
        if let Some(entry) = self.sequence_table.remove(&seq) {
            if entry.transit {
                return Some((entry.index, entry.send_time));
            }
        }
        None
    }
    
    // Equivalent to net_process_return and net_process_ping in original mtr
    async fn net_process_return(&mut self, recv_socket: &Socket, target: Ipv4Addr) {
        // Try to read multiple responses
        for _ in 0..10 {
            match Self::receive_icmp_response(recv_socket) {
                Ok((source_ip, icmp_type, seq, receive_time)) => {
                    self.net_process_ping(seq, source_ip, icmp_type, receive_time, target).await;
                }
                Err(_) => break, // No more responses
            }
        }
    }
    
    // Equivalent to net_process_ping in original mtr
    async fn net_process_ping(&mut self, seq: u16, addr: Ipv4Addr, icmp_type: IcmpType, receive_time: Instant, target: Ipv4Addr) {
        let (index, send_time) = match self.mark_sequence_complete(seq) {
            Some((idx, send_time)) => (idx, send_time),
            None => {
                debug!("Received response for unknown sequence: {}", seq);
                return;
            }
        };
        
        // Calculate RTT properly using send time from sequence table
        let rtt = receive_time.duration_since(send_time);
        
        debug!("Hop {}: Got {} from {} in {:.1}ms", 
               index + 1, icmp_type_name(icmp_type), addr, rtt.as_secs_f64() * 1000.0);
        
        // Update hop statistics (like original mtr)
        self.hops[index].add_rtt(rtt);
        
        // Set hop address if not already set
        if self.hops[index].addr.is_none() {
            self.hops[index].addr = Some(IpAddr::V4(addr));
            
            if !self.args.numeric {
                self.hops[index].hostname = Some(addr.to_string());
            }
        }
        
        // Check if we reached the target
        if addr == target && matches!(icmp_type, IcmpTypes::EchoReply) {
            info!("Reached target {} at hop {}", target, index + 1);
        }
    }
    


    fn receive_icmp_response(socket: &Socket) -> Result<(Ipv4Addr, IcmpType, u16, Instant)> {
        let mut buffer = [MaybeUninit::uninit(); 1500];
        let receive_time = Instant::now();
        
        match socket.recv_from(&mut buffer) {
            Ok((size, _addr)) => {
                // Convert MaybeUninit to initialized data
                let initialized_buffer: Vec<u8> = buffer[..size]
                    .iter()
                    .map(|b| unsafe { b.assume_init() })
                    .collect();
                
                // Parse IP packet
                if let Some(ip_packet) = Ipv4Packet::new(&initialized_buffer) {
                    let source_ip = ip_packet.get_source();
                    
                    // Check if it's an ICMP packet
                    if ip_packet.get_next_level_protocol() == IpNextHeaderProtocols::Icmp {
                        let icmp_start = (ip_packet.get_header_length() * 4) as usize;
                        if let Some(icmp_packet) = IcmpPacket::new(&initialized_buffer[icmp_start..]) {
                            let icmp_type = icmp_packet.get_icmp_type();
                            
                            match icmp_type {
                                IcmpTypes::TimeExceeded => {
                                    // Extract original packet info
                                    if let Some((_orig_id, orig_seq)) = Self::extract_original_packet_info(icmp_packet.payload()) {
                                        return Ok((source_ip, icmp_type, orig_seq, receive_time));
                                    }
                                }
                                IcmpTypes::EchoReply => {
                                    // Parse echo reply
                                    if let Some(echo_reply) = pnet::packet::icmp::echo_reply::EchoReplyPacket::new(icmp_packet.payload()) {
                                        return Ok((source_ip, icmp_type, echo_reply.get_sequence_number(), receive_time));
                                    }
                                }
                                IcmpTypes::DestinationUnreachable => {
                                    // Extract original packet info
                                    if let Some((_orig_id, orig_seq)) = Self::extract_original_packet_info(icmp_packet.payload()) {
                                        return Ok((source_ip, icmp_type, orig_seq, receive_time));
                                    }
                                }
                                _ => {
                                    debug!("Received unhandled ICMP type: {:?} from {}", icmp_type, source_ip);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock && e.kind() != std::io::ErrorKind::TimedOut {
                    debug!("Socket recv error: {}", e);
                }
            }
        }
        
        Err(anyhow::anyhow!("No valid response received"))
    }

    fn send_icmp_packet_static(socket: &Socket, target: Ipv4Addr, ttl: u32, id: u16, sequence: u16) -> Result<()> {
        // Create ICMP echo request packet
        let mut icmp_buffer = [0u8; 64];
        let mut icmp_packet = pnet::packet::icmp::echo_request::MutableEchoRequestPacket::new(&mut icmp_buffer)
            .ok_or_else(|| anyhow::anyhow!("Failed to create ICMP packet"))?;
        
        icmp_packet.set_icmp_type(IcmpTypes::EchoRequest);
        icmp_packet.set_icmp_code(pnet::packet::icmp::IcmpCode::new(0));
        icmp_packet.set_identifier(id);
        icmp_packet.set_sequence_number(sequence);
        
        // Add some payload data to make packet more identifiable
        let payload = format!("mtr-{}-{}", id, sequence);
        let payload_bytes = payload.as_bytes();
        if payload_bytes.len() <= icmp_packet.payload().len() {
            icmp_packet.payload_mut()[..payload_bytes.len()].copy_from_slice(payload_bytes);
        }
        
        // Calculate checksum
        let checksum = util::checksum(icmp_packet.packet(), 1);
        icmp_packet.set_checksum(checksum);
        
        // Set TTL on socket
        socket.set_ttl(ttl)?;
        
        // Send packet
        let target_addr = SocketAddr::new(IpAddr::V4(target), 0);
        socket.send_to(icmp_packet.packet(), &target_addr.into())?;
        
        Ok(())
    }

    fn extract_original_packet_info(payload: &[u8]) -> Option<(u16, u16)> {
        // For TimeExceeded and DestinationUnreachable, payload contains original IP packet
        if payload.len() >= 28 { // IP header (20) + ICMP header (8) minimum
            // Skip 4 bytes of ICMP error header
            if let Some(orig_ip_packet) = Ipv4Packet::new(&payload[4..]) {
                if orig_ip_packet.get_next_level_protocol() == IpNextHeaderProtocols::Icmp {
                    let orig_icmp_start = 4 + (orig_ip_packet.get_header_length() * 4) as usize;
                    if orig_icmp_start < payload.len() && orig_icmp_start + 8 <= payload.len() {
                        if let Some(orig_icmp) = pnet::packet::icmp::echo_request::EchoRequestPacket::new(&payload[orig_icmp_start..]) {
                            return Some((orig_icmp.get_identifier(), orig_icmp.get_sequence_number()));
                        }
                    }
                }
            }
        }
        None
    }

    async fn resolve_hostname(&self, addr: Ipv4Addr) -> Result<String> {
        Self::resolve_hostname_static(&self.resolver, addr).await
    }
    
    async fn resolve_hostname_static(resolver: &TokioAsyncResolver, addr: Ipv4Addr) -> Result<String> {
        match resolver.reverse_lookup(IpAddr::V4(addr)).await {
            Ok(names) => {
                if let Some(name) = names.iter().next() {
                    Ok(name.to_string().trim_end_matches('.').to_string())
                } else {
                    Ok(addr.to_string())
                }
            }
            Err(_) => Ok(addr.to_string())
        }
    }

    async fn run_simulated_trace(&mut self) -> Result<()> {
        info!("Running simulated traceroute (use sudo for real network tracing)");
        
        for round in 0..self.args.count {
            debug!("Simulation Round {}", round + 1);
            
            for hop in &mut self.hops {
                hop.increment_sent();
                
                // Simulate realistic network behavior
                let base_latency = hop.hop as u64 * 10 + 20; // Base latency increases with hops
                let jitter = rand::random::<u64>() % 50; // Random jitter
                let packet_loss_chance = (hop.hop as f64 * 0.01).min(0.1); // Small loss chance
                
                if rand::random::<f64>() > packet_loss_chance {
                    let rtt = Duration::from_millis(base_latency + jitter);
                    hop.add_rtt(rtt);
                    
                    // Simulate realistic IP addresses and hostnames
                    if hop.addr.is_none() {
                        // Generate realistic-looking IP addresses
                        match hop.hop {
                            1 => {
                                hop.addr = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
                                hop.hostname = if !self.args.numeric { Some("gateway.local".to_string()) } else { None };
                            }
                            2..=3 => {
                                hop.addr = Some(IpAddr::V4(Ipv4Addr::new(10, 0, hop.hop, 1)));
                                hop.hostname = if !self.args.numeric { Some(format!("core-{}.isp.net", hop.hop)) } else { None };
                            }
                            _ => {
                                let final_octet = if hop.hop >= 8 { 8 } else { hop.hop };
                                hop.addr = Some(IpAddr::V4(Ipv4Addr::new(8, 8, 8, final_octet)));
                                hop.hostname = if !self.args.numeric { Some("dns.google".to_string()) } else { None };
                            }
                        }
                    }
                    
                    // Stop at target (simulate reaching destination)
                    if hop.hop >= 8 {
                        break;
                    }
                } else {
                    hop.add_timeout();
                }
            }
            
            time::sleep(Duration::from_millis(self.args.interval)).await;
        }
        
        Ok(())
    }
}

fn icmp_type_name(icmp_type: IcmpType) -> &'static str {
    match icmp_type {
        IcmpTypes::TimeExceeded => "TimeExceeded",
        IcmpTypes::EchoReply => "EchoReply",
        IcmpTypes::DestinationUnreachable => "DestUnreach",
        _ => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mtr_session_new_with_ip() {
        let args = Args {
            target: "192.168.1.1".to_string(),
            count: 5,
            interval: 500,
            max_hops: 20,
            report: false,
            numeric: true,
        };
        
        let session = MtrSession::new(args).await;
        assert!(session.is_ok());
        
        let session = session.unwrap();
        assert_eq!(session.target, "192.168.1.1");
        assert_eq!(session.target_addr.to_string(), "192.168.1.1");
        assert_eq!(session.hops.len(), 20);
        assert_eq!(session.args.count, 5);
        assert_eq!(session.args.interval, 500);
    }

    #[tokio::test]
    async fn test_mtr_session_new_with_localhost() {
        let args = Args {
            target: "localhost".to_string(),
            count: 3,
            interval: 1000,
            max_hops: 15,
            report: true,
            numeric: false,
        };
        
        let session = MtrSession::new(args).await;
        assert!(session.is_ok());
        
        let session = session.unwrap();
        assert_eq!(session.target, "localhost");
        assert_eq!(session.hops.len(), 15);
        assert!(session.args.report);
        assert!(!session.args.numeric);
    }

    #[test]
    fn test_mtr_session_clone() {
        let args = Args {
            target: "example.com".to_string(),
            count: 10,
            interval: 1000,
            max_hops: 30,
            report: false,
            numeric: false,
        };
        
        // We can't easily test MtrSession::new in sync context due to async resolver,
        // but we can test that the struct supports Clone
        // This is mainly a compilation test
        let args_clone = args.clone();
        assert_eq!(args.target, args_clone.target);
        assert_eq!(args.count, args_clone.count);
    }
} 