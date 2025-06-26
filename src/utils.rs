// Individual modules import what they need

/// Time conversion utilities
pub mod time {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// Convert Duration to milliseconds as f64
    pub fn duration_to_ms_f64(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    /// Convert Duration to milliseconds as u64
    pub fn duration_to_ms_u64(duration: Duration) -> u64 {
        (duration.as_secs_f64() * 1000.0) as u64
    }

    /// Convert Duration to microseconds for high precision
    pub fn duration_to_us_f64(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1_000_000.0
    }

    /// Convert Duration to microseconds as u64
    pub fn duration_to_us_u64(duration: Duration) -> u64 {
        duration.as_micros() as u64
    }

    /// Convert Duration to nanoseconds for maximum precision
    pub fn duration_to_ns_u128(duration: Duration) -> u128 {
        duration.as_nanos()
    }

    /// Format duration as milliseconds with one decimal place
    pub fn format_duration_ms(duration: Duration) -> String {
        format!("{:.1}", duration_to_ms_f64(duration))
    }

    /// Format duration with high precision (microseconds)
    pub fn format_duration_us(duration: Duration) -> String {
        let us = duration_to_us_f64(duration);
        if us < 1000.0 {
            format!("{:.1}μs", us)
        } else {
            format!("{:.1}ms", us / 1000.0)
        }
    }

    /// Format optional duration as milliseconds with one decimal place, or "???" if None
    pub fn format_optional_duration_ms(duration: Option<Duration>) -> String {
        duration
            .map(|d| format_duration_ms(d))
            .unwrap_or_else(|| "???".to_string())
    }

    /// Format optional duration with high precision
    pub fn format_optional_duration_us(duration: Option<Duration>) -> String {
        duration
            .map(|d| format_duration_us(d))
            .unwrap_or_else(|| "???".to_string())
    }

    /// Get high-precision monotonic timestamp
    pub fn get_monotonic_timestamp() -> Instant {
        Instant::now()
    }

    /// Get system timestamp with nanosecond precision
    pub fn get_system_timestamp_ns() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    }

    /// Calculate precise elapsed time with overflow protection
    pub fn calculate_precise_elapsed(start: Instant) -> Duration {
        start.elapsed()
    }

    /// Calculate timing jitter between consecutive measurements
    pub fn calculate_timing_jitter(current: Duration, previous: Duration) -> Duration {
        if current > previous {
            current - previous
        } else {
            previous - current
        }
    }

    /// Moving average for timing smoothing
    pub fn calculate_timing_moving_average(values: &[Duration], window_size: usize) -> Option<Duration> {
        if values.is_empty() || window_size == 0 {
            return None;
        }

        let window_size = window_size.min(values.len());
        let recent_values = &values[values.len() - window_size..];
        
        let sum_nanos: u128 = recent_values
            .iter()
            .map(|d| d.as_nanos())
            .sum();
        
        let avg_nanos = sum_nanos / window_size as u128;
        Some(Duration::from_nanos(avg_nanos as u64))
    }

    /// Exponential moving average for timing with configurable alpha
    pub fn calculate_timing_ema(current: Duration, previous_ema: Option<Duration>, alpha: f64) -> Duration {
        match previous_ema {
            None => current,
            Some(prev) => {
                let current_ns = current.as_nanos() as f64;
                let prev_ns = prev.as_nanos() as f64;
                let ema_ns = alpha * current_ns + (1.0 - alpha) * prev_ns;
                Duration::from_nanos(ema_ns as u64)
            }
        }
    }

    /// Detect timing anomalies (spikes or drops)
    pub fn detect_timing_anomaly(current: Duration, baseline: Duration, threshold_factor: f64) -> bool {
        let current_ns = current.as_nanos() as f64;
        let baseline_ns = baseline.as_nanos() as f64;
        
        if baseline_ns == 0.0 {
            return false;
        }
        
        let ratio = current_ns / baseline_ns;
        ratio > threshold_factor || ratio < (1.0 / threshold_factor)
    }

    /// Calculate timing percentiles for performance analysis
    pub fn calculate_timing_percentile(values: &mut [Duration], percentile: f64) -> Option<Duration> {
        if values.is_empty() || percentile < 0.0 || percentile > 100.0 {
            return None;
        }

        values.sort_unstable();
        let index = ((percentile / 100.0) * (values.len() - 1) as f64).round() as usize;
        values.get(index).copied()
    }

    /// Real-time timing statistics
    #[derive(Debug, Clone)]
    pub struct TimingStats {
        pub count: usize,
        pub min: Duration,
        pub max: Duration,
        pub sum: Duration,
        pub mean: Duration,
        pub variance: f64,
        pub stddev: Duration,
        pub last_update: Instant,
    }

    impl TimingStats {
        pub fn new() -> Self {
            Self {
                count: 0,
                min: Duration::MAX,
                max: Duration::ZERO,
                sum: Duration::ZERO,
                mean: Duration::ZERO,
                variance: 0.0,
                stddev: Duration::ZERO,
                last_update: Instant::now(),
            }
        }

        pub fn update(&mut self, duration: Duration) {
            self.count += 1;
            self.sum += duration;
            self.min = self.min.min(duration);
            self.max = self.max.max(duration);
            self.mean = self.sum / self.count as u32;
            self.last_update = Instant::now();

            // Calculate variance incrementally for efficiency
            if self.count > 1 {
                let mean_ns = self.mean.as_nanos() as f64;
                let duration_ns = duration.as_nanos() as f64;
                let diff = duration_ns - mean_ns;
                self.variance = ((self.count - 1) as f64 * self.variance + diff * diff) / self.count as f64;
                self.stddev = Duration::from_nanos(self.variance.sqrt() as u64);
            }
        }
    }
}

/// Mathematical utilities
pub mod math {
    /// Clamp a value between min and max
    pub fn clamp_f64(value: f64, min: f64, max: f64) -> f64 {
        value.clamp(min, max)
    }

    /// Safe array indexing with bounds checking
    pub fn safe_array_index(ratio: f64, array_len: usize) -> usize {
        let index = (ratio * (array_len - 1) as f64).round() as usize;
        index.min(array_len - 1)
    }

    /// Calculate ratio for array indexing (0.0 to 1.0)
    pub fn calculate_ratio(value: f64, max_value: f64) -> f64 {
        if max_value > 0.0 {
            (value / max_value).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Calculate logarithmic ratio for scaling
    pub fn calculate_log_ratio(value: f64, min_value: f64, max_value: f64) -> f64 {
        if min_value <= 0.0 || max_value <= min_value || value <= 0.0 {
            return 0.0;
        }
        
        let log_value = value.ln();
        let log_min = min_value.ln();
        let log_max = max_value.ln();
        
        ((log_value - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
    }

    /// Calculate standard deviation from a collection of values
    pub fn calculate_stddev(values: &[f64], mean: f64) -> f64 {
        if values.len() <= 1 {
            return 0.0;
        }
        
        let variance = values
            .iter()
            .map(|&value| {
                let diff = value - mean;
                diff * diff
            })
            .sum::<f64>()
            / (values.len() - 1) as f64;
        
        variance.sqrt()
    }

    /// Clamp ratio to 0.0-1.0 range (common pattern)
    pub fn clamp_ratio(ratio: f64) -> f64 {
        ratio.clamp(0.0, 1.0)
    }

    /// Ensure value doesn't exceed a maximum with safety bounds
    pub fn min_with_safety<T: Ord>(value: T, max_value: T) -> T {
        value.min(max_value)
    }

    /// Ensure value meets a minimum threshold
    pub fn max_with_minimum<T: Ord>(value: T, min_value: T) -> T {
        value.max(min_value)
    }
}

/// Network address utilities
pub mod network {
    use std::net::IpAddr;

    /// Format IP address as string, or "???" if None
    pub fn format_optional_ip(addr: Option<IpAddr>) -> String {
        addr.map(|a| a.to_string())
            .unwrap_or_else(|| "???".to_string())
    }

    /// Format hostname with fallback to IP address
    pub fn format_hostname_with_fallback(
        hostname: Option<String>,
        addr: Option<IpAddr>,
    ) -> String {
        hostname.unwrap_or_else(|| format_optional_ip(addr))
    }

    /// Truncate hostname to specified length with ellipsis
    pub fn truncate_hostname(hostname: &str, max_len: usize) -> String {
        if hostname.len() > max_len {
            let truncated_len = max_len.saturating_sub(3); // Reserve space for "..."
            format!("{}...", &hostname[..truncated_len])
        } else {
            hostname.to_string()
        }
    }
}

/// Layout and sizing utilities
pub mod layout {
    /// Calculate constrained width with min/max bounds
    pub fn constrain_width(width: u16, min_width: u16, max_width: u16) -> u16 {
        width.saturating_sub(4).max(min_width).min(max_width)
    }

    /// Calculate popup dimensions constrained by area
    pub fn calculate_popup_dimensions(
        area_width: u16,
        area_height: u16,
        preferred_width: u16,
        preferred_height: u16,
    ) -> (u16, u16) {
        let width = preferred_width.min(area_width.saturating_sub(4));
        let height = preferred_height.min(area_height.saturating_sub(4));
        (width, height)
    }

    /// Calculate centered position for popup
    pub fn center_popup(area_width: u16, area_height: u16, popup_width: u16, popup_height: u16) -> (u16, u16) {
        let x = area_width.saturating_sub(popup_width) / 2;
        let y = area_height.saturating_sub(popup_height) / 2;
        (x, y)
    }
}

/// Formatting utilities
pub mod format {
    /// Format percentage with one decimal place
    pub fn format_percentage(value: f64) -> String {
        format!("{:.1}%", value)
    }

    /// Format number with specified decimal places and width
    pub fn format_number_padded(value: f64, width: usize, decimals: usize) -> String {
        format!("{:width$.decimals$}", value, width = width, decimals = decimals)
    }

    /// Format count with specified width
    pub fn format_count_padded(value: usize, width: usize) -> String {
        format!("{:width$}", value, width = width)
    }
}

/// ICMP packet utilities
pub mod icmp {
    use crate::Result;

    /// Calculate ICMP checksum
    pub fn calculate_checksum(packet: &[u8]) -> u16 {
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

    /// Construct basic ICMP echo request packet
    pub fn construct_icmp_packet(seq: u16, id: u16) -> Result<Vec<u8>> {
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
        let checksum = calculate_checksum(&packet);
        packet[2..4].copy_from_slice(&checksum.to_be_bytes());

        Ok(packet)
    }

    /// Construct basic ICMPv6 echo request packet
    pub fn construct_icmp6_packet(seq: u16, id: u16) -> Result<Vec<u8>> {
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
        Ok(packet)
    }

    /// Extract sequence number from ICMP packet
    pub fn extract_sequence_from_packet(packet: &[u8]) -> Option<u16> {
        if packet.len() >= 8 {
            Some(u16::from_be_bytes([packet[6], packet[7]]))
        } else {
            None
        }
    }
}

/// Visualization utilities
pub mod visualization {
    /// Get sparkline character based on ratio (0.0 to 1.0)
    pub fn get_sparkline_char(ratio: f64) -> char {
        let level = (super::math::clamp_ratio(ratio) * 8.0).round() as usize;
        let chars = ['▁', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        chars[level.min(chars.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_time_utils() {
        let duration = Duration::from_millis(1500);
        assert_eq!(time::duration_to_ms_f64(duration), 1500.0);
        assert_eq!(time::duration_to_ms_u64(duration), 1500);
        assert_eq!(time::format_duration_ms(duration), "1500.0");
    }

    #[test]
    fn test_math_utils() {
        assert_eq!(math::clamp_f64(1.5, 0.0, 1.0), 1.0);
        assert_eq!(math::safe_array_index(0.5, 10), 5); // (0.5 * 9).round() = 4.5.round() = 5
        assert_eq!(math::calculate_ratio(50.0, 100.0), 0.5);
    }

    #[test]
    fn test_network_utils() {
        use std::net::{IpAddr, Ipv4Addr};
        let addr = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(network::format_optional_ip(addr), "192.168.1.1");
        assert_eq!(network::format_optional_ip(None), "???");
        
        let long_hostname = "very-long-hostname-that-should-be-truncated";
        assert_eq!(network::truncate_hostname(long_hostname, 20), "very-long-hostnam...");
    }

    #[test]
    fn test_layout_utils() {
        assert_eq!(layout::constrain_width(100, 20, 60), 60);
        assert_eq!(layout::constrain_width(10, 20, 60), 20);
        
        let (width, height) = layout::calculate_popup_dimensions(100, 50, 80, 40);
        assert_eq!(width, 80);
        assert_eq!(height, 40);
    }

    #[test]
    fn test_format_utils() {
        assert_eq!(format::format_percentage(12.345), "12.3%");
        assert_eq!(format::format_number_padded(123.45, 8, 2), "  123.45");
    }

    #[test]
    fn test_icmp_utils() {
        let packet = icmp::construct_icmp_packet(1234, 5678).unwrap();
        assert_eq!(packet.len(), 8);
        assert_eq!(packet[0], 8); // ICMP Echo Request
        assert_eq!(icmp::extract_sequence_from_packet(&packet), Some(1234));
    }

    #[test]
    fn test_visualization_utils() {
        // Test basic sparkline character functionality
        assert_eq!(visualization::get_sparkline_char(0.0), '▁');
        assert_eq!(visualization::get_sparkline_char(0.5), '▄'); // Middle character
        assert_eq!(visualization::get_sparkline_char(1.0), '█'); // Max character
        assert_eq!(visualization::get_sparkline_char(1.5), '█'); // Should clamp to 1.0
        assert_eq!(visualization::get_sparkline_char(-0.1), '▁'); // Should clamp to 0.0
    }

    #[test]
    fn test_additional_math_utils() {
        // Test clamp_ratio
        assert_eq!(math::clamp_ratio(0.5), 0.5);
        assert_eq!(math::clamp_ratio(-0.1), 0.0);
        assert_eq!(math::clamp_ratio(1.5), 1.0);

        // Test min/max with safety
        assert_eq!(math::min_with_safety(5, 10), 5);
        assert_eq!(math::min_with_safety(15, 10), 10);
        assert_eq!(math::max_with_minimum(5, 10), 10);
        assert_eq!(math::max_with_minimum(15, 10), 15);
    }

    #[test]
    fn test_high_precision_time_utils() {
        // Test microsecond conversion
        let duration = Duration::from_micros(1500);
        assert_eq!(time::duration_to_us_f64(duration), 1500.0);
        assert_eq!(time::duration_to_us_u64(duration), 1500);
        
        // Test nanosecond conversion
        let duration_ns = Duration::from_nanos(1_234_567);
        assert_eq!(time::duration_to_ns_u128(duration_ns), 1_234_567);
        
        // Test high-precision formatting
        let small_duration = Duration::from_micros(500);
        assert_eq!(time::format_duration_us(small_duration), "500.0μs");
        
        let large_duration = Duration::from_millis(1500);
        assert_eq!(time::format_duration_us(large_duration), "1500.0ms");
    }

    #[test]
    fn test_timing_jitter_calculation() {
        let current = Duration::from_millis(120);
        let previous = Duration::from_millis(100);
        let jitter = time::calculate_timing_jitter(current, previous);
        assert_eq!(jitter, Duration::from_millis(20));
        
        // Test reverse order
        let jitter_reverse = time::calculate_timing_jitter(previous, current);
        assert_eq!(jitter_reverse, Duration::from_millis(20));
    }

    #[test]
    fn test_timing_ema() {
        let first = Duration::from_millis(100);
        let ema1 = time::calculate_timing_ema(first, None, 0.1);
        assert_eq!(ema1, first);
        
        let second = Duration::from_millis(200);
        let ema2 = time::calculate_timing_ema(second, Some(ema1), 0.1);
        // EMA should be closer to first value due to low alpha
        assert!(ema2 > first && ema2 < second);
    }

    #[test]
    fn test_timing_anomaly_detection() {
        let baseline = Duration::from_millis(100);
        let normal = Duration::from_millis(110);
        let spike = Duration::from_millis(300);
        
        assert!(!time::detect_timing_anomaly(normal, baseline, 2.0));
        assert!(time::detect_timing_anomaly(spike, baseline, 2.0));
    }

    #[test]
    fn test_timing_stats() {
        let mut stats = time::TimingStats::new();
        
        stats.update(Duration::from_millis(100));
        assert_eq!(stats.count, 1);
        assert_eq!(stats.min, Duration::from_millis(100));
        assert_eq!(stats.max, Duration::from_millis(100));
        
        stats.update(Duration::from_millis(50));
        assert_eq!(stats.count, 2);
        assert_eq!(stats.min, Duration::from_millis(50));
        assert_eq!(stats.max, Duration::from_millis(100));
        assert_eq!(stats.mean, Duration::from_millis(75));
    }

    #[test]
    fn test_timing_moving_average() {
        let values = vec![
            Duration::from_millis(100),
            Duration::from_millis(110),
            Duration::from_millis(120),
            Duration::from_millis(130),
        ];
        
        let avg = time::calculate_timing_moving_average(&values, 3);
        assert!(avg.is_some());
        // Average of last 3 values: (110 + 120 + 130) / 3 = 120
        assert_eq!(avg.unwrap(), Duration::from_millis(120));
    }

    #[test]
    fn test_timing_precision_improvements() {
        // Test high-precision microsecond timing
        let ultra_fast = Duration::from_micros(100); // 100μs - typical for localhost
        assert_eq!(time::duration_to_us_f64(ultra_fast), 100.0);
        assert_eq!(time::format_duration_us(ultra_fast), "100.0μs");
        
        let sub_ms = Duration::from_micros(750); // 0.75ms
        assert_eq!(time::format_duration_us(sub_ms), "750.0μs");
        
        let ms_range = Duration::from_millis(5); // 5ms - show ms formatting
        assert_eq!(time::format_duration_us(ms_range), "5.0ms");
        
        // Test that we no longer have 10ms quantization
        let precise_timing = Duration::from_nanos(123_456); // 123.456μs
        assert_eq!(time::duration_to_us_f64(precise_timing), 123.456);
        assert_eq!(time::duration_to_ns_u128(precise_timing), 123_456);
        
        // Verify sub-millisecond precision detection
        assert!(time::duration_to_us_f64(ultra_fast) < 1000.0); // Should use μs format
        assert!(time::duration_to_us_f64(ms_range) >= 1000.0);  // Should use ms format
    }
} 