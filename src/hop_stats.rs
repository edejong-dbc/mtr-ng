use std::{collections::VecDeque, net::IpAddr, time::Duration};

#[derive(Debug, Clone)]
pub enum PacketOutcome {
    Received(Duration), // RTT
    Lost,              // Timeout/no response
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
    pub rtts: VecDeque<Duration>,
    pub packet_history: VecDeque<PacketOutcome>, // Chronological packet outcomes
    pub loss_percent: f64,
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
            rtts: VecDeque::with_capacity(100),
            packet_history: VecDeque::with_capacity(100),
            loss_percent: 0.0,
        }
    }

    pub fn add_rtt(&mut self, rtt: Duration) {
        self.received += 1;
        self.last_rtt = Some(rtt);
        self.rtts.push_back(rtt);
        
        // Add to packet history chronologically
        self.packet_history.push_back(PacketOutcome::Received(rtt));
        
        if self.rtts.len() > 100 {
            self.rtts.pop_front();
        }
        
        if self.packet_history.len() > 100 {
            self.packet_history.pop_front();
        }

        // Update statistics
        if self.best_rtt.is_none() || rtt < self.best_rtt.unwrap() {
            self.best_rtt = Some(rtt);
        }
        if self.worst_rtt.is_none() || rtt > self.worst_rtt.unwrap() {
            self.worst_rtt = Some(rtt);
        }

        let sum: Duration = self.rtts.iter().sum();
        self.avg_rtt = Some(sum / self.rtts.len() as u32);
        
        self.update_loss_percent();
    }

    pub fn add_timeout(&mut self) {
        // Add lost packet to chronological history
        self.packet_history.push_back(PacketOutcome::Lost);
        
        if self.packet_history.len() > 100 {
            self.packet_history.pop_front();
        }
        
        self.update_loss_percent();
    }

    pub fn update_loss_percent(&mut self) {
        if self.sent > 0 {
            self.loss_percent = ((self.sent - self.received) as f64 / self.sent as f64) * 100.0;
        }
    }

    pub fn increment_sent(&mut self) {
        self.sent += 1;
        self.update_loss_percent();
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
} 