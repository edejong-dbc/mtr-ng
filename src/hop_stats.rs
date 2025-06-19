use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub enum PacketOutcome {
    Received(Duration), // RTT
    Lost,               // Timeout/no response
    Pending,            // Sent but no response yet
}

#[derive(Debug, Clone)]
pub struct AlternatePath {
    pub addr: IpAddr,
    pub hostname: Option<String>,
    pub frequency: usize,
    pub last_seen: Instant,
    pub last_rtt: Option<Duration>,
    pub avg_rtt: Option<Duration>,
}

impl AlternatePath {
    pub fn new(addr: IpAddr) -> Self {
        Self {
            addr,
            hostname: None,
            frequency: 1,
            last_seen: Instant::now(),
            last_rtt: None,
            avg_rtt: None,
        }
    }

    pub fn update(&mut self, rtt: Duration) {
        self.frequency += 1;
        self.last_seen = Instant::now();
        self.last_rtt = Some(rtt);
        // Simple running average
        self.avg_rtt = Some(rtt);
    }
}

#[derive(Debug, Clone)]
pub struct HopStats {
    pub hop: u8,
    pub addr: Option<IpAddr>,
    pub hostname: Option<String>,
    pub sent: usize,
    pub received: usize,
    pub last_rtt: Option<Duration>,
    pub best_rtt: Option<Duration>,
    pub worst_rtt: Option<Duration>,
    pub avg_rtt: Option<Duration>,
    pub ema_rtt: Option<Duration>, // Exponentially smoothed average RTT
    pub jitter_avg: Option<Duration>, // Mean jitter (average of jitter values)
    pub last_jitter: Option<Duration>, // Last calculated jitter value
    pub jitters: VecDeque<Duration>, // Store jitter values for average calculation
    pub rtts: VecDeque<Duration>,
    pub packet_history: VecDeque<PacketOutcome>, // Chronological packet outcomes
    pub loss_percent: f64,
    // Exponential smoothing factor (0.0 to 1.0)
    // Higher values = more responsive to recent changes
    // Lower values = more stable, less sensitive to spikes
    // Typical values: 0.1-0.3 for network monitoring
    pub ema_alpha: f64,

    // Multi-path tracking
    pub alternate_paths: HashMap<IpAddr, AlternatePath>,
    pub path_frequency: HashMap<IpAddr, usize>,
}

impl HopStats {
    pub fn new(hop: u8) -> Self {
        Self {
            hop,
            addr: None,
            hostname: None,
            sent: 0,
            received: 0,
            last_rtt: None,
            best_rtt: None,
            worst_rtt: None,
            avg_rtt: None,
            ema_rtt: None,
            jitter_avg: None,
            last_jitter: None,
            jitters: VecDeque::with_capacity(100),
            rtts: VecDeque::with_capacity(100),
            packet_history: VecDeque::with_capacity(100),
            loss_percent: 0.0,
            ema_alpha: 0.1,
            alternate_paths: HashMap::new(),
            path_frequency: HashMap::new(),
        }
    }

    /// Track an RTT from a specific address, handling multi-path logic
    pub fn add_rtt_from_addr(&mut self, addr: IpAddr, rtt: Duration) {
        // Update path frequency tracking
        *self.path_frequency.entry(addr).or_insert(0) += 1;

        // Determine if this is the primary path
        let is_primary = self.addr.is_none()
            || self.addr == Some(addr)
            || self.path_frequency.get(&addr).unwrap_or(&0)
                > self
                    .path_frequency
                    .get(&self.addr.unwrap_or(addr))
                    .unwrap_or(&0);

        if is_primary {
            // Update primary path stats
            self.addr = Some(addr);
            self.add_rtt(rtt);
        } else {
            // Track as alternate path
            let alt_path = self
                .alternate_paths
                .entry(addr)
                .or_insert_with(|| AlternatePath::new(addr));
            alt_path.update(rtt);
            let alt_frequency = alt_path.frequency; // Save for logging

            // For alternate paths, we still need to count the received packet
            self.received += 1;
            self.update_loss_percent();

            tracing::debug!(
                "Alternate path detected: hop={}, primary={:?}, alternate={}, frequency={}",
                self.hop,
                self.addr,
                addr,
                alt_frequency
            );
        }
    }

    /// Get all alternate paths sorted by frequency
    pub fn get_alternate_paths(&self) -> Vec<&AlternatePath> {
        let mut paths: Vec<_> = self.alternate_paths.values().collect();
        paths.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        paths
    }

    /// Check if this hop has multiple paths
    pub fn has_multiple_paths(&self) -> bool {
        !self.alternate_paths.is_empty()
    }

    /// Get total frequency across all paths
    pub fn get_total_frequency(&self) -> usize {
        let primary_freq = self
            .path_frequency
            .get(&self.addr.unwrap_or(IpAddr::from([0, 0, 0, 0])))
            .unwrap_or(&0);
        let alt_freq: usize = self.alternate_paths.values().map(|p| p.frequency).sum();
        primary_freq + alt_freq
    }

    /// Calculate percentage for an alternate path
    pub fn get_path_percentage(&self, path: &AlternatePath) -> f64 {
        let total = self.get_total_frequency();
        if total > 0 {
            (path.frequency as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Calculate percentage for the primary path
    pub fn get_primary_path_percentage(&self) -> f64 {
        let total = self.get_total_frequency();
        if total > 0 {
            let primary_freq = self
                .path_frequency
                .get(&self.addr.unwrap_or(IpAddr::from([0, 0, 0, 0])))
                .unwrap_or(&0);
            (*primary_freq as f64 / total as f64) * 100.0
        } else {
            100.0 // If no frequency data, assume 100%
        }
    }

    /// Set hostname for a specific address
    pub fn set_hostname_for_addr(&mut self, addr: IpAddr, hostname: String) {
        if Some(addr) == self.addr {
            self.hostname = Some(hostname);
        } else if let Some(alt_path) = self.alternate_paths.get_mut(&addr) {
            alt_path.hostname = Some(hostname);
        }
    }

    pub fn add_rtt(&mut self, rtt: Duration) {
        self.received += 1;

        // Calculate jitter BEFORE updating last_rtt (need previous value)
        // Jitter = |current_rtt - previous_rtt|
        if let Some(prev_rtt) = self.last_rtt {
            let jitter = if rtt > prev_rtt {
                rtt - prev_rtt
            } else {
                prev_rtt - rtt
            };

            self.last_jitter = Some(jitter);
            self.jitters.push_back(jitter);

            // Maintain capacity limit for jitter values
            if self.jitters.len() > 100 {
                self.jitters.pop_front();
            }

            // Calculate mean jitter
            let jitter_sum: Duration = self.jitters.iter().sum();
            self.jitter_avg = Some(jitter_sum / self.jitters.len() as u32);

            tracing::debug!(
                "Jitter: hop={}, current_jitter={:.1}ms, avg_jitter={:.1}ms",
                self.hop,
                jitter.as_secs_f64() * 1000.0,
                self.jitter_avg.unwrap().as_secs_f64() * 1000.0
            );
        }

        self.last_rtt = Some(rtt);
        self.rtts.push_back(rtt);

        // Find the last pending packet and mark it as received
        for outcome in self.packet_history.iter_mut().rev() {
            if matches!(outcome, PacketOutcome::Pending) {
                *outcome = PacketOutcome::Received(rtt);
                break;
            }
        }

        if self.rtts.len() > 100 {
            self.rtts.pop_front();
        }

        tracing::debug!(
            "add_rtt: hop={}, received={}, rtt={:.1}ms",
            self.hop,
            self.received,
            rtt.as_secs_f64() * 1000.0
        );

        // Update statistics
        if self.best_rtt.is_none() || rtt < self.best_rtt.unwrap() {
            self.best_rtt = Some(rtt);
        }
        if self.worst_rtt.is_none() || rtt > self.worst_rtt.unwrap() {
            self.worst_rtt = Some(rtt);
        }

        // Calculate arithmetic average
        let sum: Duration = self.rtts.iter().sum();
        self.avg_rtt = Some(sum / self.rtts.len() as u32);

        // Calculate exponential moving average
        // EMA = α * current_value + (1 - α) * previous_ema
        // For first value, EMA = current_value
        match self.ema_rtt {
            None => {
                // First RTT measurement - initialize EMA
                self.ema_rtt = Some(rtt);
            }
            Some(prev_ema) => {
                // Apply exponential smoothing formula
                let rtt_ms = rtt.as_secs_f64() * 1000.0;
                let prev_ema_ms = prev_ema.as_secs_f64() * 1000.0;
                let new_ema_ms = self.ema_alpha * rtt_ms + (1.0 - self.ema_alpha) * prev_ema_ms;
                self.ema_rtt = Some(Duration::from_secs_f64(new_ema_ms / 1000.0));
            }
        }

        self.update_loss_percent();
    }

    pub fn add_timeout(&mut self) {
        // Find the oldest pending packet and mark it as lost
        for outcome in self.packet_history.iter_mut() {
            if matches!(outcome, PacketOutcome::Pending) {
                *outcome = PacketOutcome::Lost;
                break;
            }
        }

        tracing::debug!(
            "add_timeout: hop={}, packet_history.len()={}",
            self.hop,
            self.packet_history.len()
        );

        self.update_loss_percent();
    }

    pub fn update_loss_percent(&mut self) {
        if self.sent > 0 {
            // Ensure received can't exceed sent to prevent overflow
            let actual_received = self.received.min(self.sent);
            self.loss_percent = ((self.sent - actual_received) as f64 / self.sent as f64) * 100.0;
        }
    }

    pub fn increment_sent(&mut self) {
        self.sent += 1;

        // Add pending packet to chronological history when sent
        self.packet_history.push_back(PacketOutcome::Pending);

        if self.packet_history.len() > 100 {
            self.packet_history.pop_front();
        }

        tracing::debug!(
            "increment_sent: hop={}, sent={}, packet_history.len()={}",
            self.hop,
            self.sent,
            self.packet_history.len()
        );

        self.update_loss_percent();
    }

    /// Set the exponential smoothing factor (alpha)
    /// Values closer to 1.0 make the average more responsive to recent changes
    /// Values closer to 0.0 make the average more stable and less sensitive to spikes
    /// Typical values for network monitoring: 0.1-0.3
    pub fn set_ema_alpha(&mut self, alpha: f64) {
        self.ema_alpha = alpha.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hop_stats_new() {
        let hop = HopStats::new(5);
        assert_eq!(hop.hop, 5);
        assert_eq!(hop.sent, 0);
        assert_eq!(hop.received, 0);
        assert_eq!(hop.loss_percent, 0.0);
        assert!(hop.addr.is_none());
        assert!(hop.hostname.is_none());
        assert!(hop.last_rtt.is_none());
        assert!(hop.best_rtt.is_none());
        assert!(hop.worst_rtt.is_none());
        assert!(hop.avg_rtt.is_none());
        assert!(hop.ema_rtt.is_none());
        assert!(hop.jitter_avg.is_none());
        assert!(hop.last_jitter.is_none());
        assert!(hop.rtts.is_empty());
    }

    #[test]
    fn test_hop_stats_add_rtt() {
        let mut hop = HopStats::new(1);

        // Add first RTT
        let rtt1 = Duration::from_millis(100);
        hop.add_rtt(rtt1);

        assert_eq!(hop.received, 1);
        assert_eq!(hop.last_rtt, Some(rtt1));
        assert_eq!(hop.best_rtt, Some(rtt1));
        assert_eq!(hop.worst_rtt, Some(rtt1));
        assert_eq!(hop.avg_rtt, Some(rtt1));
        assert_eq!(hop.ema_rtt, Some(rtt1)); // First value initializes EMA
        assert!(hop.jitter_avg.is_none()); // No jitter yet (need 2+ samples)
        assert!(hop.last_jitter.is_none());
        assert_eq!(hop.rtts.len(), 1);

        // Add second RTT (better)
        let rtt2 = Duration::from_millis(50);
        hop.add_rtt(rtt2);

        assert_eq!(hop.received, 2);
        assert_eq!(hop.last_rtt, Some(rtt2));
        assert_eq!(hop.best_rtt, Some(rtt2));
        assert_eq!(hop.worst_rtt, Some(rtt1));
        assert_eq!(hop.avg_rtt, Some(Duration::from_millis(75))); // Average of 100 and 50
        assert_eq!(hop.rtts.len(), 2);

        // Add third RTT (worse)
        let rtt3 = Duration::from_millis(200);
        hop.add_rtt(rtt3);

        assert_eq!(hop.received, 3);
        assert_eq!(hop.last_rtt, Some(rtt3));
        assert_eq!(hop.best_rtt, Some(rtt2));
        assert_eq!(hop.worst_rtt, Some(rtt3));
        assert_eq!(hop.rtts.len(), 3);
    }

    #[test]
    fn test_hop_stats_loss_calculation() {
        let mut hop = HopStats::new(1);

        // No packets sent yet
        assert_eq!(hop.loss_percent, 0.0);

        // Send 10 packets, receive 8
        for _ in 0..8 {
            hop.increment_sent();
            hop.add_rtt(Duration::from_millis(100));
        }
        for _ in 0..2 {
            hop.increment_sent();
            hop.add_timeout();
        }

        assert_eq!(hop.sent, 10);
        assert_eq!(hop.received, 8);
        assert_eq!(hop.loss_percent, 20.0); // 2 lost out of 10 = 20%
    }

    #[test]
    fn test_hop_stats_rtts_capacity_limit() {
        let mut hop = HopStats::new(1);

        // Add more than 100 RTTs to test capacity limit
        for i in 0..150 {
            hop.add_rtt(Duration::from_millis(i as u64));
        }

        assert_eq!(hop.rtts.len(), 100); // Should be capped at 100
        assert_eq!(hop.received, 150); // But received count should be accurate

        // The oldest RTTs should have been removed
        assert_eq!(hop.rtts.front(), Some(&Duration::from_millis(50))); // Should start from 50
        assert_eq!(hop.rtts.back(), Some(&Duration::from_millis(149))); // Should end at 149
    }

    #[test]
    fn test_hop_stats_clone() {
        let mut original = HopStats::new(3);
        original.add_rtt(Duration::from_millis(150));
        original.increment_sent();
        original.addr = Some("192.168.1.3".parse().unwrap());
        original.hostname = Some("gateway.local".to_string());

        let cloned = original.clone();

        assert_eq!(original.hop, cloned.hop);
        assert_eq!(original.sent, cloned.sent);
        assert_eq!(original.received, cloned.received);
        assert_eq!(original.addr, cloned.addr);
        assert_eq!(original.hostname, cloned.hostname);
        assert_eq!(original.last_rtt, cloned.last_rtt);
        assert_eq!(original.rtts.len(), cloned.rtts.len());
    }

    #[test]
    fn test_hop_stats_update_loss_percent_edge_cases() {
        let mut hop = HopStats::new(1);

        // Test with no packets sent
        hop.update_loss_percent();
        assert_eq!(hop.loss_percent, 0.0);

        // Test with 100% loss
        hop.sent = 5;
        hop.received = 0;
        hop.update_loss_percent();
        assert_eq!(hop.loss_percent, 100.0);

        // Test with 0% loss
        hop.received = 5;
        hop.update_loss_percent();
        assert_eq!(hop.loss_percent, 0.0);

        // Test with partial loss
        hop.sent = 10;
        hop.received = 7;
        hop.update_loss_percent();
        assert_eq!(hop.loss_percent, 30.0);
    }

    #[test]
    fn test_hop_stats_increment_sent() {
        let mut hop = HopStats::new(1);

        assert_eq!(hop.sent, 0);
        assert_eq!(hop.loss_percent, 0.0);

        hop.increment_sent();
        assert_eq!(hop.sent, 1);
        assert_eq!(hop.loss_percent, 100.0); // 1 sent, 0 received = 100% loss

        hop.add_rtt(Duration::from_millis(100)); // This also calls increment_sent internally
        assert_eq!(hop.sent, 1); // Should still be 1 since add_rtt doesn't increment sent
        assert_eq!(hop.received, 1);
        assert_eq!(hop.loss_percent, 0.0); // 1 sent, 1 received = 0% loss
    }

    #[test]
    fn test_large_rtt_values() {
        let mut hop = HopStats::new(1);

        // Test with very large RTT values
        let large_rtt = Duration::from_secs(5); // 5 seconds
        hop.add_rtt(large_rtt);

        assert_eq!(hop.best_rtt, Some(large_rtt));
        assert_eq!(hop.worst_rtt, Some(large_rtt));
        assert_eq!(hop.avg_rtt, Some(large_rtt));

        // Add a smaller RTT
        let small_rtt = Duration::from_millis(10);
        hop.add_rtt(small_rtt);

        assert_eq!(hop.best_rtt, Some(small_rtt));
        assert_eq!(hop.worst_rtt, Some(large_rtt));
    }

    #[test]
    fn test_hop_stats_add_timeout() {
        let mut hop = HopStats::new(1);

        // Add some successful RTTs first
        hop.increment_sent();
        hop.add_rtt(Duration::from_millis(100));
        hop.increment_sent();
        hop.add_rtt(Duration::from_millis(150));

        assert_eq!(hop.sent, 2);
        assert_eq!(hop.received, 2);
        assert_eq!(hop.loss_percent, 0.0);

        // Add timeout
        hop.increment_sent();
        hop.add_timeout();

        assert_eq!(hop.sent, 3);
        assert_eq!(hop.received, 2);
        assert!((hop.loss_percent - 33.333333333333336).abs() < 1e-10); // 1 lost out of 3
    }

    #[test]
    fn test_rtt_statistics_accuracy() {
        let mut hop = HopStats::new(1);

        let rtts = vec![50, 100, 75, 200, 25]; // in milliseconds

        for &rtt_ms in &rtts {
            hop.add_rtt(Duration::from_millis(rtt_ms));
        }

        assert_eq!(hop.best_rtt, Some(Duration::from_millis(25)));
        assert_eq!(hop.worst_rtt, Some(Duration::from_millis(200)));
        assert_eq!(hop.last_rtt, Some(Duration::from_millis(25))); // Last added

        // Average should be (50 + 100 + 75 + 200 + 25) / 5 = 90
        assert_eq!(hop.avg_rtt, Some(Duration::from_millis(90)));
    }

    #[test]
    fn test_exponential_moving_average() {
        let mut hop = HopStats::new(1);
        hop.set_ema_alpha(0.5); // Use 0.5 for easier testing (50% weight to new values)

        // First RTT should initialize EMA
        hop.add_rtt(Duration::from_millis(100));
        assert_eq!(hop.ema_rtt, Some(Duration::from_millis(100)));

        // Second RTT: EMA = 0.5 * 200 + 0.5 * 100 = 150
        hop.add_rtt(Duration::from_millis(200));
        let ema_ms = (hop.ema_rtt.unwrap().as_secs_f64() * 1000.0).round() as u64;
        assert_eq!(ema_ms, 150);

        // Third RTT: EMA = 0.5 * 100 + 0.5 * 150 = 125
        hop.add_rtt(Duration::from_millis(100));
        let ema_ms = (hop.ema_rtt.unwrap().as_secs_f64() * 1000.0).round() as u64;
        assert_eq!(ema_ms, 125);
    }

    #[test]
    fn test_ema_alpha_clamping() {
        let mut hop = HopStats::new(1);

        // Test values outside valid range are clamped
        hop.set_ema_alpha(-0.5);
        assert_eq!(hop.ema_alpha, 0.0);

        hop.set_ema_alpha(1.5);
        assert_eq!(hop.ema_alpha, 1.0);

        hop.set_ema_alpha(0.3);
        assert_eq!(hop.ema_alpha, 0.3);
    }

    #[test]
    fn test_jitter_calculation() {
        let mut hop = HopStats::new(1);

        // First RTT - no jitter yet
        hop.add_rtt(Duration::from_millis(100));
        assert!(hop.jitter_avg.is_none());
        assert!(hop.last_jitter.is_none());

        // Second RTT - first jitter calculation
        hop.add_rtt(Duration::from_millis(120));
        assert_eq!(hop.last_jitter, Some(Duration::from_millis(20))); // |120 - 100| = 20
        assert_eq!(hop.jitter_avg, Some(Duration::from_millis(20))); // Only one jitter value
        assert_eq!(hop.jitters.len(), 1);

        // Third RTT - jitter decreases
        hop.add_rtt(Duration::from_millis(110));
        assert_eq!(hop.last_jitter, Some(Duration::from_millis(10))); // |110 - 120| = 10
        let expected_avg = (20 + 10) / 2; // Average of 20ms and 10ms = 15ms
        assert_eq!(hop.jitter_avg, Some(Duration::from_millis(expected_avg)));
        assert_eq!(hop.jitters.len(), 2);

        // Fourth RTT - larger jitter spike
        hop.add_rtt(Duration::from_millis(150));
        assert_eq!(hop.last_jitter, Some(Duration::from_millis(40))); // |150 - 110| = 40
                                                                      // Average of jitter values: (20 + 10 + 40) / 3 = 23.333... ms
        let expected_avg_ms = (hop.jitter_avg.unwrap().as_secs_f64() * 1000.0).round() as u64;
        assert_eq!(expected_avg_ms, 23); // Rounded to nearest ms
        assert_eq!(hop.jitters.len(), 3);
    }
}
