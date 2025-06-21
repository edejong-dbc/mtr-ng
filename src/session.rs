use crate::{Args, HopStats, Result};
use crate::probe::{ProbeEngine, ProbeResponse, IcmpResponseType};
use anyhow::anyhow;
use hickory_resolver::{config::{ResolverConfig, ResolverOpts}, TokioAsyncResolver};
use rand;

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::time;
use tracing::{debug, info, warn};

const MIN_SEQUENCE: u16 = 33000;
const MAX_SEQUENCE: u16 = 65535;

// Add callback type for real-time updates
pub type UpdateCallback = Arc<dyn Fn() + Send + Sync>;



#[derive(Debug, Clone)]
pub struct SequenceEntry {
    pub index: usize,       // hop index (like original mtr)
    pub transit: bool,      // is this sequence in transit?
    pub saved_seq: u32,     // saved sequence for this host
    pub send_time: Instant, // when packet was sent
}

#[derive(Clone)]
pub struct MtrSession {
    pub target: String,
    pub target_addr: IpAddr,
    pub hops: Vec<HopStats>,
    pub args: Args,
    pub resolver: TokioAsyncResolver,
    pub packet_id: u16,
    pub next_sequence: u16,
    pub sequence_table: HashMap<u16, SequenceEntry>, // sequence -> entry (like original mtr)
    pub batch_at: usize,  // current hop index being sent (like original mtr)
    pub num_hosts: usize, // number of active hops
    pub update_callback: Option<UpdateCallback>, // callback for real-time updates
}

impl MtrSession {
    pub async fn new(args: Args) -> Result<Self> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

        // Resolve target hostname to IP
        let target_addr = if let Ok(ip) = args.target.parse::<IpAddr>() {
            ip
        } else {
            let response = resolver.lookup_ip(&args.target).await?;
            response
                .iter()
                .next()
                .ok_or_else(|| anyhow!("Failed to resolve hostname"))?
        };

        let mut hops: Vec<HopStats> = (1..=args.max_hops).map(HopStats::new).collect();

        // Configure EMA alpha for all hops from command line args
        for hop in &mut hops {
            hop.set_ema_alpha(args.ema_alpha);
        }
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
            batch_at: 0,   // Start at hop 1 (index 0)
            num_hosts: 10, // Initial estimate
            update_callback: None,
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
        if self.args.simulate {
            info!("Running in simulation mode (--simulate flag enabled)");
            return self.run_simulated_trace().await;
        }

        // Try to create ProbeEngine for modern ICMP handling
        match ProbeEngine::new() {
            Ok(probe_engine) => {
                info!("Using ProbeEngine for real traceroute");
                self.run_mtr_algorithm_with_probe_engine(target, probe_engine)
                    .await
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to create ProbeEngine: {}. This usually means insufficient permissions. \
                    Try running with sudo, or use --simulate for demo mode.", e
                );
            }
        }
    }



    // Modern ProbeEngine implementation 
    async fn run_mtr_algorithm_with_probe_engine(
        &mut self,
        target: Ipv4Addr,
        mut probe_engine: ProbeEngine,
    ) -> Result<()> {
        info!("Starting MTR algorithm with ProbeEngine");
        let mut round = 0;

        loop {
            if let Some(count) = self.args.count {
                if round >= count {
                    break;
                }
            }

            let round_start = Instant::now();

            // Send probes for all active hops (like net_send_batch)
            let restart = self.net_send_batch_with_probe_engine(target, &mut probe_engine).await?;

            // Collect responses efficiently
            let collect_duration = Duration::from_millis(self.args.interval);
            self.net_process_return_with_probe_engine(&mut probe_engine, target, collect_duration).await;

            if restart {
                round += 1;
                if let Some(count) = self.args.count {
                    debug!("Completed round {}/{}, restarting batch", round, count);
                } else {
                    debug!("Completed round {} (continuous), restarting batch", round);
                }
                
                // Only wait for remaining interval time if we're not done
                if self.args.count.is_none() || round < self.args.count.unwrap() {
                    let elapsed = round_start.elapsed();
                    let target_interval = Duration::from_millis(self.args.interval);
                    if elapsed < target_interval {
                        tokio::time::sleep(target_interval - elapsed).await;
                    }
                }
            }
        }

        Ok(())
    }
    
    // ProbeEngine-based equivalent of net_send_batch - send to all hops in parallel
    async fn net_send_batch_with_probe_engine(
        &mut self,
        target: Ipv4Addr,
        probe_engine: &mut ProbeEngine,
    ) -> Result<bool> {
        // Send probes to all hops in parallel (like simulation mode)
        // This is the correct MTR algorithm - not incremental discovery
        let max_hops = if self.num_hosts > 0 {
            self.num_hosts.min(self.args.max_hops as usize)
            } else {
            10.min(self.args.max_hops as usize) // Start with reasonable number
        };

        // Send all probes rapidly in succession
        for i in 0..max_hops {
            self.net_send_query_with_probe_engine(target, probe_engine, i)?;
        }

        // Always restart after sending batch (that's how MTR works)
        Ok(true)
    }

    // ProbeEngine-based equivalent of net_send_query
    fn net_send_query_with_probe_engine(
        &mut self,
        target: Ipv4Addr,
        probe_engine: &mut ProbeEngine,
        index: usize,
    ) -> Result<()> {
        let time_to_live = (index + 1) as u8;
        let seq = self.prepare_sequence(index);
        let send_time = Instant::now();

        self.save_sequence_with_send_time(index, seq, send_time);

        let target_addr = std::net::SocketAddr::from((target, 33434)); // Standard traceroute port for UDP/TCP
        let timeout = Duration::from_millis(200); // Short timeout per individual probe (like original MTR)

        // Send probe using ProbeEngine with selected protocol
        probe_engine.send_probe_with_protocol(
            index, 
            target_addr, 
            time_to_live, 
            timeout,
            self.args.protocol
        )?;

        debug!("Sent {:?} probe to hop {} (TTL={}), seq={}", 
               self.args.protocol, index + 1, time_to_live, seq);
        Ok(())
    }

    // ProbeEngine-based equivalent of net_process_return - optimized
    async fn net_process_return_with_probe_engine(
        &mut self,
        probe_engine: &mut ProbeEngine,
        target: Ipv4Addr,
        _collect_duration: Duration,
    ) {
        // Use efficient response collection with reasonable timeout
        // Don't wait the full interval - collect quickly and exit when done
        let start_collect = Instant::now();
        let max_wait = Duration::from_millis(200); // Much shorter maximum wait
        let mut total_responses = 0;
        let mut no_response_cycles = 0;

        while start_collect.elapsed() < max_wait && no_response_cycles < 20 {
            let batch_responses = probe_engine.collect_responses().unwrap_or_default();
            total_responses += batch_responses.len();

            if batch_responses.is_empty() {
                no_response_cycles += 1;
                // Very short delay between checks
                tokio::time::sleep(Duration::from_millis(10)).await;
            } else {
                no_response_cycles = 0; // Reset counter when we get responses
                
                for response in batch_responses {
                    self.process_probe_response(response, target).await;
                }
            }
        }
        
        debug!("Collected {} responses in {:?}", total_responses, start_collect.elapsed());
    }

    // Process individual probe responses
    async fn process_probe_response(&mut self, response: ProbeResponse, target: Ipv4Addr) {
        let hop_index = response.hop;
        
        if hop_index >= self.hops.len() {
            return; // Invalid hop index
        }

        match response.icmp_type {
            IcmpResponseType::TimeExceeded => {
                // Intermediate hop response - update RTT and address
                self.hops[hop_index].add_rtt_from_addr(response.source_addr, response.rtt);
                debug!("Got TimeExceeded from {} for hop {} (RTT: {:?})", 
                       response.source_addr, hop_index + 1, response.rtt);
                
                // DNS lookup if needed
        if !self.args.numeric {
                    self.perform_dns_lookup(hop_index, response.source_addr).await;
                }
            }
            IcmpResponseType::EchoReply => {
                // Direct response - update stats and check if target
                self.hops[hop_index].add_rtt_from_addr(response.source_addr, response.rtt);
                
                // Check if we reached the target
                if let IpAddr::V4(source_ipv4) = response.source_addr {
                    if source_ipv4 == target {
                        info!("Reached target {} at hop {}", target, hop_index + 1);
                    }
                }
                
                // DNS lookup if needed
                if !self.args.numeric {
                    self.perform_dns_lookup(hop_index, response.source_addr).await;
                }
            }
            IcmpResponseType::DestinationUnreachable => {
                // ICMP error - mark hop with error but still update address for display
                self.hops[hop_index].set_icmp_error();
                // Still set the address so it shows up instead of "???"
                if self.hops[hop_index].addr.is_none() {
                    self.hops[hop_index].addr = Some(response.source_addr);
                }
                debug!("Got DestinationUnreachable from {} for hop {}", 
                       response.source_addr, hop_index + 1);
            }
            IcmpResponseType::Timeout => {
                // Timeout - just increment timeout count
                debug!("Timeout for hop {}", hop_index + 1);
            }
        }

        // Trigger real-time UI update when a response arrives
        if let Some(ref callback) = self.update_callback {
            callback();
        }
    }

    // DNS lookup functionality
    async fn perform_dns_lookup(&mut self, hop_index: usize, addr: IpAddr) {
        if hop_index >= self.hops.len() {
            return;
        }

        if let Ok(lookup_result) = self
            .resolver
            .reverse_lookup(addr)
            .await
        {
            if let Some(hostname) = lookup_result.iter().next() {
                let hostname_str = hostname.to_string();
                if hostname_str != addr.to_string() {
                    debug!("Resolved {} to {}", addr, hostname_str);
                    self.hops[hop_index].set_hostname_for_addr(addr, hostname_str);
                }
            }
        }
    }

    // ProbeEngine-based sequence management
    fn prepare_sequence(&mut self, index: usize) -> u16 {
        let seq = self.next_sequence;
        self.next_sequence += 1;
        if self.next_sequence == MAX_SEQUENCE {
            self.next_sequence = MIN_SEQUENCE;
        }
        self.hops[index].increment_sent();
        seq
    }

    fn save_sequence_with_send_time(&mut self, index: usize, seq: u16, send_time: Instant) {
        let entry = SequenceEntry {
            index,
            transit: true,
            saved_seq: self.hops[index].sent as u32,
            send_time,
        };
        self.sequence_table.insert(seq, entry);
    }

    async fn run_simulated_trace(&mut self) -> Result<()> {
        info!("Running simulated traceroute (use sudo for real network tracing)");

        for round in 0..self.args.count.unwrap_or(10) {
            debug!("Simulation Round {}", round + 1);

            for hop in &mut self.hops {
                hop.increment_sent();

                // Simulate realistic network behavior
                let base_latency = hop.hop as u64 * 10 + 20; // Base latency increases with hops
                let jitter = rand::random::<u64>() % 50; // Random jitter
                let packet_loss_chance = (hop.hop as f64 * 0.05).min(0.25); // Higher loss chance for testing

                if rand::random::<f64>() > packet_loss_chance {
                    let rtt = Duration::from_millis(base_latency + jitter);
                    hop.add_rtt(rtt);

                    // Simulate realistic IP addresses and hostnames
                    if hop.addr.is_none() {
                        // Generate realistic-looking IP addresses
                        match hop.hop {
                            1 => {
                                hop.addr = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
                                hop.hostname = if !self.args.numeric {
                                    Some("gateway.local".to_string())
                                } else {
                                    None
                                };
                            }
                            2..=3 => {
                                hop.addr = Some(IpAddr::V4(Ipv4Addr::new(10, 0, hop.hop, 1)));
                                hop.hostname = if !self.args.numeric {
                                    Some(format!("core-{}.isp.net", hop.hop))
                                } else {
                                    None
                                };
                            }
                            _ => {
                                let final_octet = if hop.hop >= 8 { 8 } else { hop.hop };
                                hop.addr = Some(IpAddr::V4(Ipv4Addr::new(8, 8, 8, final_octet)));
                                hop.hostname = if !self.args.numeric {
                                    Some("dns.google".to_string())
                                } else {
                                    None
                                };
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

    pub fn set_update_callback(&mut self, callback: UpdateCallback) {
        self.update_callback = Some(callback);
    }

         // Real-time updates with ProbeEngine integration
    pub async fn run_trace_with_realtime_updates(
        session_arc: std::sync::Arc<std::sync::Mutex<Self>>,
    ) -> Result<()> {
         // Extract target and args from session
        let (target_addr, args) = {
            let session = session_arc.lock().unwrap();
            (session.target_addr, session.args.clone())
        };

        info!("Starting real-time trace to {}", target_addr);

         if args.simulate {
             info!("Running in simulation mode (--simulate flag enabled)");
             return Self::run_simulated_trace_realtime(session_arc, args).await;
         }

        match target_addr {
             IpAddr::V4(ipv4) => {
                 // Try real network tracing first
                 match ProbeEngine::new() {
                     Ok(probe_engine) => {
                         info!("Using ProbeEngine for real-time traceroute");
                         Self::run_real_trace_realtime(session_arc, ipv4, probe_engine, args).await
                     }
                     Err(e) => {
                         warn!("Failed to create ProbeEngine: {}. Falling back to simulation.", e);
                         Self::run_simulated_trace_realtime(session_arc, args).await
                     }
                 }
             }
            IpAddr::V6(_) => {
                warn!("IPv6 not yet implemented, falling back to simulation");
                Self::run_simulated_trace_realtime(session_arc, args).await
            }
        }
    }

     async fn run_real_trace_realtime(
        session_arc: std::sync::Arc<std::sync::Mutex<Self>>,
        target: Ipv4Addr,
         probe_engine: ProbeEngine,
        args: Args,
    ) -> Result<()> {
         info!("Starting real network trace with channels (real-time UI)");
         
         // Create channel for communication between probe task and UI
         #[allow(unused_mut)]
         let (response_tx, mut response_rx) = mpsc::unbounded_channel::<ProbeResponse>();
         
         // Clone session for probe task
         let probe_session_arc = Arc::clone(&session_arc);
         let probe_args = args.clone();
         
         // Spawn probe task that runs independently
         let probe_handle = tokio::spawn(async move {
             Self::run_probe_task(probe_session_arc, target, probe_engine, probe_args, response_tx).await
         });
         
         // UI task processes responses from channel without blocking probe timing
         let ui_handle = tokio::spawn(async move {
             Self::run_ui_response_processor(session_arc, response_rx).await
         });
         
         // Wait for both tasks
         let (probe_result, ui_result) = tokio::try_join!(probe_handle, ui_handle)?;
         probe_result?;
         ui_result?;
         
         Ok(())
     }
     
     // Probe task - continuously sends probes and async listens for responses
     #[allow(unused_mut)]
     async fn run_probe_task(
         _session_arc: std::sync::Arc<std::sync::Mutex<Self>>,
        target: Ipv4Addr,
         mut probe_engine: ProbeEngine,
        args: Args,
         response_tx: mpsc::UnboundedSender<ProbeResponse>,
    ) -> Result<()> {
         let max_hops = 10.min(args.max_hops as usize);
         info!("Probe task starting with {} max hops", max_hops);
         
         // Spawn continuous response listener task
         #[allow(unused_mut)]
         let (probe_tx, mut probe_rx) = mpsc::unbounded_channel();
         let listener_response_tx = response_tx.clone();
         
         let listener_handle = tokio::spawn(async move {
             Self::run_response_listener(probe_engine, probe_rx, listener_response_tx).await
         });
         
         // Main probe sending loop
         let sender_handle = tokio::spawn(async move {
        let mut round = 0;
             
        loop {
            if let Some(count) = args.count {
                if round >= count {
                    break;
                }
                 }
                 
                 // Send all probes for this round
                 for i in 0..max_hops {
                     let dest = SocketAddr::new(target.into(), 0);
                     let ttl = (i + 1) as u8;
                     let timeout = Duration::from_millis(5000);
                     
                     // Send probe request to listener task
                     if probe_tx.send((i, dest, ttl, timeout, round)).is_err() {
                         return Ok::<(), anyhow::Error>(());
                     }
                 }
                 
                 debug!("Sent {} probes for round {}", max_hops, round + 1);
                 round += 1;
                 
                 tokio::time::sleep(Duration::from_millis(args.interval)).await;
             }
             
             info!("Probe sender completed {} rounds", round);
        Ok(())
         });
         
         // Wait for both tasks
         let (listener_result, sender_result) = tokio::try_join!(listener_handle, sender_handle)?;
         listener_result?;
         sender_result?;
         
         Ok(())
     }
     
     // Continuous async response listener - tracks sequence numbers properly
     async fn run_response_listener(
         mut probe_engine: ProbeEngine,
         mut probe_rx: mpsc::UnboundedReceiver<(usize, SocketAddr, u8, Duration, usize)>, // (hop, dest, ttl, timeout, round)
         response_tx: mpsc::UnboundedSender<ProbeResponse>,
     ) -> Result<()> {
         use std::collections::HashMap;
         
         let mut sent_sequences: HashMap<u16, (usize, usize)> = HashMap::new(); // seq -> (hop, round)
         
         loop {
             tokio::select! {
                 // Handle probe send requests
                 probe_request = probe_rx.recv() => {
                     if let Some((hop, dest, ttl, timeout, round)) = probe_request {
                         match probe_engine.send_probe(hop, dest, ttl, timeout) {
                             Ok(seq) => {
                                 sent_sequences.insert(seq, (hop, round));
                                 debug!("Sent probe: hop={}, round={}, seq={}", hop + 1, round + 1, seq);
                             }
                             Err(e) => debug!("Failed to send probe: {}", e),
                         }
                     } else {
                         // Sender dropped, time to exit
                         break;
                     }
                 }
                 
                 // Continuously collect responses
                 _ = tokio::time::sleep(Duration::from_millis(10)) => {
                     let responses = probe_engine.collect_responses().unwrap_or_default();
                     
                     for response in responses {
                         // Check if this sequence belongs to a known round
                         if let Some((expected_hop, round)) = sent_sequences.remove(&response.seq) {
                             if expected_hop == response.hop {
                                 debug!("Valid response: hop={}, round={}, seq={}, rtt={:?}", 
                                       response.hop + 1, round + 1, response.seq, response.rtt);
                             } else {
                                 debug!("WARNING: Hop mismatch - expected {}, got {}", expected_hop + 1, response.hop + 1);
                             }
                             
                             if response_tx.send(response).is_err() {
                                 return Ok(());
                             }
                         } else {
                             debug!("OUT-OF-ORDER/LATE: seq={}, hop={}, rtt={:?} - no matching sent probe", 
                                   response.seq, response.hop + 1, response.rtt);
                         }
                     }
                 }
             }
         }
         
         info!("Response listener finished");
         Ok(())
     }
     
     // UI response processor - handles responses without affecting probe timing
     async fn run_ui_response_processor(
         session_arc: std::sync::Arc<std::sync::Mutex<Self>>,
         mut response_rx: mpsc::UnboundedReceiver<ProbeResponse>,
     ) -> Result<()> {
         let mut probe_count = 0;
         
         while let Some(response) = response_rx.recv().await {
             let mut session = session_arc.lock().unwrap();
             let hop_index = response.hop;
             
             if hop_index < session.hops.len() {
                 // Increment sent count for accurate statistics
                 if response.icmp_type != IcmpResponseType::Timeout {
                     probe_count += 1;
                     if probe_count % 10 == 1 { // Every new round
                         for hop in &mut session.hops {
                             hop.increment_sent();
                         }
                     }
                 }
                 
                 match response.icmp_type {
                     IcmpResponseType::TimeExceeded | IcmpResponseType::EchoReply => {
                         // RTT is calculated in ProbeEngine when response arrives - no timing corruption!
                         session.hops[hop_index].add_rtt_from_addr(response.source_addr, response.rtt);
                         debug!("UI: Hop {} RTT: {:?} from {}", hop_index + 1, response.rtt, response.source_addr);
                     }
                     IcmpResponseType::DestinationUnreachable => {
                         session.hops[hop_index].set_icmp_error();
                         if session.hops[hop_index].addr.is_none() {
                             session.hops[hop_index].addr = Some(response.source_addr);
                         }
                         debug!("UI: Hop {} destination unreachable from {}", hop_index + 1, response.source_addr);
                     }
                     IcmpResponseType::Timeout => {
                         debug!("UI: Hop {} timeout", hop_index + 1);
                     }
                 }
                 
                 // Trigger UI update after processing each response
                 if let Some(ref callback) = session.update_callback {
                            callback();
                        }
                    }
                }
         
         info!("UI response processor finished");
         Ok(())
    }

    async fn run_simulated_trace_realtime(
        session_arc: std::sync::Arc<std::sync::Mutex<Self>>,
        args: Args,
    ) -> Result<()> {
        info!("Running simulated traceroute (real-time)");

        // Extract the numeric flag once to avoid borrow conflicts
        let numeric = args.numeric;

        for round in 0..args.count.unwrap_or(1000) {
            debug!("Simulation Round {}", round + 1);

            // Extract callback and hop updates in separate blocks to avoid borrow conflicts
            let should_update_ui = {
                    let mut session = session_arc.lock().unwrap();
                for hop in &mut session.hops {
                        hop.increment_sent();

                    let base_latency = hop.hop as u64 * 10 + 20;
                    let jitter = rand::random::<u64>() % 50;
                    let packet_loss_chance = (hop.hop as f64 * 0.05).min(0.25);

                        if rand::random::<f64>() > packet_loss_chance {
                            let rtt = Duration::from_millis(base_latency + jitter);
                        hop.add_rtt(rtt);

                            if hop.addr.is_none() {
                            match hop.hop {
                                    1 => {
                                        hop.addr = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
                                        hop.hostname = if !numeric {
                                            Some("gateway.local".to_string())
                                        } else {
                                            None
                                        };
                                    }
                                    2..=3 => {
                                    hop.addr = Some(IpAddr::V4(Ipv4Addr::new(10, 0, hop.hop, 1)));
                                        hop.hostname = if !numeric {
                                        Some(format!("core-{}.isp.net", hop.hop))
                                        } else {
                                            None
                                        };
                                    }
                                    _ => {
                                    let final_octet = if hop.hop >= 8 { 8 } else { hop.hop };
                                    hop.addr = Some(IpAddr::V4(Ipv4Addr::new(8, 8, 8, final_octet)));
                                        hop.hostname = if !numeric {
                                            Some("dns.google".to_string())
                                        } else {
                                            None
                                        };
                                    }
                                }
                            }

                        if hop.hop >= 8 {
                            break;
                            }
                        } else {
                            hop.add_timeout();
                    }
                }
                
                // Return whether to update UI (true if callback exists)
                session.update_callback.is_some()
            };

            // Trigger UI update in separate block to avoid borrow conflicts
            if should_update_ui {
                let session = session_arc.lock().unwrap();
                if let Some(ref callback) = session.update_callback {
                    callback();
                }
            }

            tokio::time::sleep(Duration::from_millis(args.interval)).await;
        }

        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mtr_session_new_with_ip() {
        let args = Args {
            target: "192.168.1.1".to_string(),
            count: Some(5),
            interval: 500,
            max_hops: 20,
            report: false,
            numeric: true,
            sparkline_scale: crate::SparklineScale::Logarithmic,
            ema_alpha: 0.1,
            fields: None,
            show_all: false,
            simulate: false,
            protocol: crate::args::ProbeProtocol::Icmp,
        };

        let session = MtrSession::new(args).await;
        assert!(session.is_ok());

        let session = session.unwrap();
        assert_eq!(session.target, "192.168.1.1");
        assert_eq!(session.target_addr.to_string(), "192.168.1.1");
        assert_eq!(session.hops.len(), 20);
        assert_eq!(session.args.count, Some(5));
        assert_eq!(session.args.interval, 500);
    }

    #[tokio::test]
    async fn test_mtr_session_new_with_localhost() {
        let args = Args {
            target: "localhost".to_string(),
            count: Some(3),
            interval: 1000,
            max_hops: 15,
            report: true,
            numeric: false,
            sparkline_scale: crate::SparklineScale::Logarithmic,
            ema_alpha: 0.1,
            fields: None,
            show_all: false,
            simulate: false,
            protocol: crate::args::ProbeProtocol::Icmp,
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
            count: Some(10),
            interval: 1000,
            max_hops: 30,
            report: false,
            numeric: false,
            sparkline_scale: crate::SparklineScale::Logarithmic,
            ema_alpha: 0.1,
            fields: None,
            show_all: false,
            simulate: false,
            protocol: crate::args::ProbeProtocol::Icmp,
        };

        // We can't easily test MtrSession::new in sync context due to async resolver,
        // but we can test that the struct supports Clone
        // This is mainly a compilation test
        let args_clone = args.clone();
        assert_eq!(args.target, args_clone.target);
        assert_eq!(args.count, args_clone.count);
    }
}

