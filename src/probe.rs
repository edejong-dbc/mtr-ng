//! Simplified probe engine for MTR-NG
//!
//! This provides a cleaner interface for network probing without the complex
//! cross-platform error queue handling from the original probe_unix.c

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use crate::args::ProbeProtocol;

/// Maximum MTU size for network packets
const MAX_MTU: usize = 1500;

/// Starting sequence number for probe packets
const INITIAL_SEQUENCE: u16 = 32768;

/// Types of ICMP responses we care about
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcmpResponseType {
    EchoReply,
    TimeExceeded,
    DestinationUnreachable,
    Timeout,
}

/// Information about a probe response
#[derive(Debug, Clone)]
pub struct ProbeResponse {
    pub hop: usize,
    pub seq: u16,
    pub source_addr: IpAddr,
    pub icmp_type: IcmpResponseType,
    pub rtt: Duration,
    pub send_time: Instant,
}

/// A probe that has been sent but not yet answered.
#[derive(Debug)]
struct ProbeInfo {
    hop: usize,
    sent_at: Instant,
    timeout: Duration,
}

impl ProbeInfo {
    fn timed_out(&self) -> bool {
        self.sent_at.elapsed() >= self.timeout
    }
}

/// Simplified probe engine focused on core functionality
pub struct ProbeEngine {
    next_seq: u16,
    icmp_socket: Socket,
    icmp6_socket: Option<Socket>, // IPv6 ICMP socket
    pending: HashMap<u16, ProbeInfo>,
    packet_id: u16,
}

impl ProbeEngine {
    pub fn new() -> Result<Self> {
        // Create raw ICMP socket (requires root/sudo)
        let icmp_socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))
            .context("Failed to create raw ICMP socket - need sudo/root privileges")?;
        
        icmp_socket.set_nonblocking(true)?;

        // Try to create IPv6 ICMP socket (optional)
        let icmp6_socket = Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))
            .inspect(|sock| {
                let _ = sock.set_nonblocking(true);
            })
            .ok();

        if icmp6_socket.is_some() {
            tracing::info!("IPv6 ICMP socket created successfully");
        } else {
            tracing::warn!("IPv6 ICMP socket creation failed - IPv6 support disabled");
        }

        Ok(Self {
            next_seq: INITIAL_SEQUENCE,
            icmp_socket,
            icmp6_socket,
            pending: HashMap::new(),
            packet_id: std::process::id() as u16,
        })
    }

    /// Send a probe packet with ICMP (default protocol)
    pub fn send_probe(
        &mut self,
        hop: usize,
        dst: SocketAddr,
        ttl: u8,
        timeout: Duration,
    ) -> Result<u16> {
        self.send_probe_with_protocol(hop, dst, ttl, timeout, ProbeProtocol::Icmp)
    }

    /// Send a probe packet with the specified protocol (IPv4/IPv6 aware)
    pub fn send_probe_with_protocol(
        &mut self,
        hop: usize,
        dst: SocketAddr,
        ttl: u8,
        timeout: Duration,
        protocol: ProbeProtocol,
    ) -> Result<u16> {
        let seq = self.alloc_seq();

        // Select appropriate socket based on destination address family
        let (socket, packet) = match dst {
            SocketAddr::V4(_) => {
                self.icmp_socket.set_ttl(ttl.into())?;
                let packet = match protocol {
                    ProbeProtocol::Icmp => construct_icmp_packet(seq, self.packet_id)?,
                    ProbeProtocol::Udp => {
                        tracing::debug!("Sending UDP-style probe via ICMP socket");
                        construct_icmp_packet(seq, self.packet_id)?
                    }
                    ProbeProtocol::Tcp => {
                        tracing::debug!("Sending TCP-style probe via ICMP socket");
                        construct_icmp_packet(seq, self.packet_id)?
                    }
                };
                (&self.icmp_socket, packet)
            }
            SocketAddr::V6(_) => {
                if let Some(ref icmp6_sock) = self.icmp6_socket {
                    icmp6_sock.set_ttl(ttl.into())?;
                    let packet = match protocol {
                        ProbeProtocol::Icmp => construct_icmp6_packet(seq, self.packet_id)?,
                        ProbeProtocol::Udp => {
                            tracing::debug!("Sending UDP-style probe via ICMPv6 socket");
                            construct_icmp6_packet(seq, self.packet_id)?
                        }
                        ProbeProtocol::Tcp => {
                            tracing::debug!("Sending TCP-style probe via ICMPv6 socket");  
                            construct_icmp6_packet(seq, self.packet_id)?
                        }
                    };
                    (icmp6_sock, packet)
                } else {
                    return Err(anyhow::anyhow!("IPv6 not supported - no ICMPv6 socket available"));
                }
            }
        };

        socket.send_to(&packet, &dst.into())?;

        // Track the probe
        let probe = ProbeInfo {
            hop,
            sent_at: Instant::now(),
            timeout,
        };

        self.pending.insert(seq, probe);
        
        let addr_family = match dst {
            SocketAddr::V4(_) => "IPv4",
            SocketAddr::V6(_) => "IPv6",
        };
        
        tracing::debug!("Sent {:?} probe ({}): hop={}, ttl={}, seq={}", 
                       protocol, addr_family, hop + 1, ttl, seq);

        Ok(seq)
    }

    /// Collect all available responses from IPv4 and IPv6 sockets
    pub fn collect_responses(&mut self) -> Result<Vec<ProbeResponse>> {
        let mut responses = Vec::new();
        let mut buffer = [0u8; MAX_MTU];

        // Collect all available responses from IPv4 ICMP socket
        loop {
            let mut uninit_buffer = [std::mem::MaybeUninit::<u8>::uninit(); MAX_MTU];
            match self.icmp_socket.recv_from(&mut uninit_buffer) {
                Ok((len, addr)) => {
                    // Convert MaybeUninit to initialized bytes
                    for i in 0..len {
                        buffer[i] = unsafe { uninit_buffer[i].assume_init() };
                    }
                    if let Some(response) = self.parse_icmp_response(&buffer[..len], addr)? {
                        responses.push(response);
                    }
                }
                Err(_) => break, // No more data available
            }
        }

        // Collect all available responses from IPv6 ICMP socket (if available)
        let has_ipv6_socket = self.icmp6_socket.is_some();
        if has_ipv6_socket {
            loop {
                let mut uninit_buffer = [std::mem::MaybeUninit::<u8>::uninit(); MAX_MTU];
                let recv_result = if let Some(ref icmp6_socket) = self.icmp6_socket {
                    icmp6_socket.recv_from(&mut uninit_buffer)
                } else {
                    break;
                };

                match recv_result {
                    Ok((len, addr)) => {
                        // Convert MaybeUninit to initialized bytes
                        for i in 0..len {
                            buffer[i] = unsafe { uninit_buffer[i].assume_init() };
                        }
                        if let Some(response) = self.parse_icmp6_response(&buffer[..len], addr)? {
                            responses.push(response);
                        }
                    }
                    Err(_) => break, // No more data available
                }
            }
        }

        // Check for timeouts
        let timed_out: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, probe)| probe.timed_out())
            .map(|(seq, probe)| (*seq, probe.hop, probe.sent_at))
            .collect();

        for (seq, hop, send_time) in timed_out {
            if let Some(probe) = self.pending.remove(&seq) {
                responses.push(ProbeResponse {
                    hop,
                    seq,
                    source_addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    icmp_type: IcmpResponseType::Timeout,
                    rtt: probe.timeout,
                    send_time,
                });
            }
        }

        Ok(responses)
    }



    fn alloc_seq(&mut self) -> u16 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        if self.next_seq > 60999 {
            self.next_seq = INITIAL_SEQUENCE;
        }
        seq
    }

    fn parse_icmp_response(
        &mut self,
        buf: &[u8],
        _addr: socket2::SockAddr,
    ) -> Result<Option<ProbeResponse>> {
        if buf.len() < 28 { // IP header (20) + ICMP header (8)
            return Ok(None);
        }

        // Parse IP header
        let ip_header_len = ((buf[0] & 0x0f) * 4) as usize;
        if buf.len() < ip_header_len + 8 {
            return Ok(None);
        }

        let source = Ipv4Addr::new(buf[12], buf[13], buf[14], buf[15]);
        let icmp_data = &buf[ip_header_len..];

        // Parse ICMP header
        let icmp_type = icmp_data[0];
        
        let response_type = match icmp_type {
            0 => IcmpResponseType::EchoReply,
            11 => IcmpResponseType::TimeExceeded,
            3 => IcmpResponseType::DestinationUnreachable,
            _ => return Ok(None),
        };

        // Extract sequence number
        let seq = match response_type {
            IcmpResponseType::EchoReply => {
                if icmp_data.len() >= 8 {
                    u16::from_be_bytes([icmp_data[6], icmp_data[7]])
                } else {
                    return Ok(None);
                }
            }
            IcmpResponseType::TimeExceeded | IcmpResponseType::DestinationUnreachable => {
                // Extract from original packet in ICMP payload
                if icmp_data.len() >= 36 {
                    let orig_icmp_offset = 8 + 20; // ICMP header + IP header
                    if icmp_data.len() >= orig_icmp_offset + 8 {
                        u16::from_be_bytes([
                            icmp_data[orig_icmp_offset + 6],
                            icmp_data[orig_icmp_offset + 7],
                        ])
                    } else {
                        return Ok(None);
                    }
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        };

        // Find matching probe
        if let Some(probe) = self.pending.remove(&seq) {
            let rtt = probe.sent_at.elapsed();
            Ok(Some(ProbeResponse {
                hop: probe.hop,
                seq,
                source_addr: IpAddr::V4(source),
                icmp_type: response_type,
                rtt,
                send_time: probe.sent_at,
            }))
        } else {
            Ok(None)
        }
    }

    fn parse_icmp6_response(
        &mut self,
        buf: &[u8],
        _addr: socket2::SockAddr,
    ) -> Result<Option<ProbeResponse>> {
        // ICMPv6 has a simpler header structure than IPv4
        if buf.len() < 8 { // Minimum ICMPv6 header size
            return Ok(None);
        }

        // For ICMPv6, the packet often starts directly with the ICMPv6 header
        // (no IPv6 header in raw socket read for ICMPv6)
        let icmp6_type = buf[0];
        
        let response_type = match icmp6_type {
            129 => IcmpResponseType::EchoReply,    // ICMPv6 Echo Reply
            3 => IcmpResponseType::TimeExceeded,    // ICMPv6 Time Exceeded
            1 => IcmpResponseType::DestinationUnreachable, // ICMPv6 Destination Unreachable
            _ => return Ok(None),
        };

        // Extract sequence number based on message type
        let seq = match response_type {
            IcmpResponseType::EchoReply => {
                if buf.len() >= 8 {
                    u16::from_be_bytes([buf[6], buf[7]])
                } else {
                    return Ok(None);
                }
            }
            IcmpResponseType::TimeExceeded | IcmpResponseType::DestinationUnreachable => {
                // For error messages, the original packet is embedded
                // Skip ICMPv6 header (8 bytes) + IPv6 header (40 bytes) to get to original ICMPv6
                if buf.len() >= 56 { // 8 + 40 + 8 minimum
                    let orig_icmp_offset = 8 + 40;
                    if buf.len() >= orig_icmp_offset + 8 {
                        u16::from_be_bytes([
                            buf[orig_icmp_offset + 6],
                            buf[orig_icmp_offset + 7],
                        ])
                    } else {
                        return Ok(None);
                    }
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        };

        // Extract source address from socket address
        let source_addr = match _addr.as_socket() {
            Some(SocketAddr::V6(v6_addr)) => IpAddr::V6(*v6_addr.ip()),
            Some(SocketAddr::V4(v4_addr)) => IpAddr::V4(*v4_addr.ip()), // Shouldn't happen but handle it
            None => return Ok(None),
        };

        // Find matching probe
        if let Some(probe) = self.pending.remove(&seq) {
            let rtt = probe.sent_at.elapsed();
            Ok(Some(ProbeResponse {
                hop: probe.hop,
                seq,
                source_addr,
                icmp_type: response_type,
                rtt,
                send_time: probe.sent_at,
            }))
        } else {
            Ok(None)
        }
    }
}

// Helper function to construct ICMP packet
fn construct_icmp_packet(seq: u16, id: u16) -> Result<Vec<u8>> {
    let mut packet = vec![0u8; 8];
    
    // ICMP Type (8 = Echo Request)
    packet[0] = 8;
    // ICMP Code (0)
    packet[1] = 0;
    // Checksum (0 initially, calculated later)
    packet[2] = 0;
    packet[3] = 0;
    // Identifier
    packet[4..6].copy_from_slice(&id.to_be_bytes());
    // Sequence Number
    packet[6..8].copy_from_slice(&seq.to_be_bytes());

    // Calculate checksum
    let checksum = calculate_icmp_checksum(&packet);
    packet[2..4].copy_from_slice(&checksum.to_be_bytes());

    Ok(packet)
}

fn calculate_icmp_checksum(packet: &[u8]) -> u16 {
    let mut sum = 0u32;
    
    // Sum all 16-bit words
    for chunk in packet.chunks(2) {
        if chunk.len() == 2 {
            sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
        } else {
            sum += (chunk[0] as u32) << 8;
        }
    }
    
    // Add carry
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    
    // One's complement
    !(sum as u16)
}



// Helper function to construct ICMPv6 packet
fn construct_icmp6_packet(seq: u16, id: u16) -> Result<Vec<u8>> {
    let mut packet = vec![0u8; 8];
    
    // ICMPv6 Type (128 = Echo Request)
    packet[0] = 128;
    // ICMPv6 Code (0)
    packet[1] = 0;
    // Checksum (0 initially, kernel will calculate for ICMPv6)
    packet[2] = 0;
    packet[3] = 0;
    // Identifier
    packet[4..6].copy_from_slice(&id.to_be_bytes());
    // Sequence Number
    packet[6..8].copy_from_slice(&seq.to_be_bytes());

    // Note: For ICMPv6, the kernel typically calculates the checksum
    // so we don't need to manually calculate it like we do for ICMP

    Ok(packet)
} 