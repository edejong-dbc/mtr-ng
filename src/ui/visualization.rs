//! Visualization utilities for RTT data
//!
//! This module provides sparkline generation, color management, and RTT calculation
//! utilities for the terminal user interface.

use crate::{HopStats, SparklineScale};
use ratatui::{
    style::Style,
    text::Span,
};

// ========================================
// Public Types
// ========================================

#[derive(Debug, Clone, Copy)]
pub enum ColorSupport {
    None,      // No color support
    Basic,     // 16 colors
    Extended,  // 256 colors
    TrueColor, // 24-bit RGB
}

#[derive(Debug, Clone, Copy)]
pub enum VisualizationMode {
    Sparkline, // Variable height characters (▁▂▃▄▅▆▇█)
    Heatmap,   // Full height blocks (█) with colors only
}

// ========================================
// Color Management
// ========================================

/// Color scheme functions for RTT visualization
pub mod colors {
    use super::ColorSupport;
    use ratatui::style::Color;

    pub fn get_rtt_color(ratio: f64, color_support: ColorSupport) -> (char, Color) {
        let level = (ratio.clamp(0.0, 1.0) * 8.0).round() as usize;
        let chars = ['▁', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let char = chars[level.min(chars.len() - 1)];

        let color = match color_support {
            ColorSupport::None => Color::White,
            ColorSupport::Basic => {
                let colors = [
                    Color::Green,
                    Color::Green,
                    Color::Cyan,
                    Color::Yellow,
                    Color::Yellow,
                    Color::Magenta,
                    Color::Magenta,
                    Color::Red,
                    Color::Red,
                ];
                colors[level.min(colors.len() - 1)]
            }
            ColorSupport::Extended => {
                let colors = [17, 21, 39, 75, 111, 179, 215, 208, 130];
                let index = level.min(colors.len() - 1);
                Color::Indexed(colors[index])
            }
            ColorSupport::TrueColor => {
                let colors = [
                    (0, 50, 150),
                    (0, 100, 200),
                    (50, 150, 255),
                    (100, 200, 255),
                    (150, 220, 255),
                    (255, 200, 100),
                    (255, 150, 50),
                    (220, 120, 0),
                    (150, 80, 0),
                ];
                let (r, g, b) = colors[level.min(colors.len() - 1)];
                Color::Rgb(r, g, b)
            }
        };

        (char, color)
    }

    pub fn get_smooth_gradient_color(ratio: f64, color_support: ColorSupport) -> Color {
        let ratio = ratio.clamp(0.0, 1.0);

        match color_support {
            ColorSupport::None => Color::White,
            ColorSupport::Basic => {
                let colors = [
                    Color::Blue,
                    Color::Cyan,
                    Color::Yellow,
                    Color::Magenta,
                    Color::Red,
                ];
                let index = (ratio * (colors.len() - 1) as f64).round() as usize;
                colors[index.min(colors.len() - 1)]
            }
            ColorSupport::Extended => {
                let steps = [17, 21, 33, 39, 75, 111, 179, 215];
                let index = (ratio * (steps.len() - 1) as f64).round() as usize;
                Color::Indexed(steps[index.min(steps.len() - 1)])
            }
            ColorSupport::TrueColor => {
                let (r, g, b) = if ratio < 0.5 {
                    let t = ratio * 2.0;
                    interpolate_rgb((0.0, 50.0, 150.0), (150.0, 220.0, 255.0), t)
                } else {
                    let t = (ratio - 0.5) * 2.0;
                    interpolate_rgb((150.0, 220.0, 255.0), (220.0, 120.0, 0.0), t)
                };
                Color::Rgb(r.round() as u8, g.round() as u8, b.round() as u8)
            }
        }
    }

    pub fn get_loss_color(color_support: ColorSupport) -> Color {
        match color_support {
            ColorSupport::None => Color::White,
            ColorSupport::Basic => Color::Red,
            ColorSupport::Extended => Color::Indexed(196),
            ColorSupport::TrueColor => Color::Rgb(255, 0, 0),
        }
    }

    pub fn get_pending_color(color_support: ColorSupport) -> Color {
        match color_support {
            ColorSupport::None => Color::White,
            ColorSupport::Basic => Color::Blue,
            ColorSupport::Extended => Color::Indexed(27),
            ColorSupport::TrueColor => Color::Rgb(100, 100, 255),
        }
    }

    fn interpolate_rgb(
        start: (f64, f64, f64),
        end: (f64, f64, f64),
        ratio: f64,
    ) -> (f64, f64, f64) {
        let ratio = ratio.clamp(0.0, 1.0);
        (
            start.0 + (end.0 - start.0) * ratio,
            start.1 + (end.1 - start.1) * ratio,
            start.2 + (end.2 - start.2) * ratio,
        )
    }
}

// ========================================
// RTT Calculation Utilities
// ========================================

/// Calculate ratio for RTT visualization scaling
pub fn calculate_rtt_ratio(
    rtt_ms: u64,
    global_min: u64,
    global_max: u64,
    scale: SparklineScale,
) -> f64 {
    if global_min == global_max || rtt_ms == 0 {
        return 0.0;
    }

    match scale {
        SparklineScale::Linear => (rtt_ms as f64 / global_max as f64).clamp(0.0, 1.0),
        SparklineScale::Logarithmic => {
            let log_rtt = ((rtt_ms + 1) as f64).log10();
            let log_min = ((global_min + 1) as f64).log10();
            let log_max = ((global_max + 1) as f64).log10();
            ((log_rtt - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
        }
    }
}

// ========================================
// Sparkline Generation
// ========================================

/// Generate colored sparkline spans for RTT visualization
pub fn create_sparkline_spans(
    hop: &HopStats,
    global_min_rtt: u64,
    global_max_rtt: u64,
    scale: SparklineScale,
    color_support: ColorSupport,
    max_width: usize,
) -> Vec<Span<'static>> {
    if hop.sent == 0 || max_width == 0 {
        return vec![];
    }

    let packet_outcomes: Vec<_> = hop.packet_history.iter().collect();
    if packet_outcomes.is_empty() {
        return vec![Span::raw(" ".repeat(max_width))];
    }

    let data_to_show = if packet_outcomes.len() > max_width {
        &packet_outcomes[packet_outcomes.len() - max_width..]
    } else {
        &packet_outcomes[..]
    };

    let mut spans: Vec<Span<'static>> = data_to_show
        .iter()
        .map(|outcome| match outcome {
            crate::hop_stats::PacketOutcome::Received(rtt) => {
                let rtt_ms = (rtt.as_secs_f64() * 1000.0) as u64;
                let ratio = calculate_rtt_ratio(rtt_ms, global_min_rtt, global_max_rtt, scale);
                let (char, color) = colors::get_rtt_color(ratio, color_support);
                Span::styled(char.to_string(), Style::default().fg(color))
            }
            crate::hop_stats::PacketOutcome::Lost => {
                let color = colors::get_loss_color(color_support);
                Span::styled("·".to_string(), Style::default().fg(color))
            }
            crate::hop_stats::PacketOutcome::Pending => {
                let color = colors::get_pending_color(color_support);
                Span::styled("?".to_string(), Style::default().fg(color))
            }
        })
        .collect();

    if spans.len() < max_width {
        spans.push(Span::raw(" ".repeat(max_width - spans.len())));
    }

    spans
}

/// Generate colored heatmap spans for RTT visualization (full-height blocks)
pub fn create_heatmap_spans(
    hop: &HopStats,
    global_min_rtt: u64,
    global_max_rtt: u64,
    scale: SparklineScale,
    color_support: ColorSupport,
    max_width: usize,
) -> Vec<Span<'static>> {
    if hop.sent == 0 || max_width == 0 {
        return vec![];
    }

    let packet_outcomes: Vec<_> = hop.packet_history.iter().collect();
    if packet_outcomes.is_empty() {
        return vec![Span::raw(" ".repeat(max_width))];
    }

    let data_to_show = if packet_outcomes.len() > max_width {
        &packet_outcomes[packet_outcomes.len() - max_width..]
    } else {
        &packet_outcomes[..]
    };

    let mut spans: Vec<Span<'static>> = data_to_show
        .iter()
        .map(|outcome| {
            match outcome {
                crate::hop_stats::PacketOutcome::Received(rtt) => {
                    let rtt_ms = (rtt.as_secs_f64() * 1000.0) as u64;
                    let ratio = calculate_rtt_ratio(rtt_ms, global_min_rtt, global_max_rtt, scale);
                    // Use full-height block with color based on RTT ratio
                    let color = colors::get_smooth_gradient_color(ratio, color_support);
                    Span::styled("█".to_string(), Style::default().fg(color))
                }
                crate::hop_stats::PacketOutcome::Lost => {
                    let color = colors::get_loss_color(color_support);
                    Span::styled("·".to_string(), Style::default().fg(color))
                }
                crate::hop_stats::PacketOutcome::Pending => {
                    let color = colors::get_pending_color(color_support);
                    Span::styled("?".to_string(), Style::default().fg(color))
                }
            }
        })
        .collect();

    if spans.len() < max_width {
        spans.push(Span::raw(" ".repeat(max_width - spans.len())));
    }

    spans
}

// ========================================
// Terminal Capability Detection
// ========================================

/// Detect terminal color support capabilities
pub fn detect_color_support() -> ColorSupport {
    use std::env;

    // Check for explicit color support environment variables
    if let Ok(colorterm) = env::var("COLORTERM") {
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return ColorSupport::TrueColor;
        }
    }

    // Check TERM environment variable
    if let Ok(term) = env::var("TERM") {
        if term.contains("256color") || term.contains("256") {
            return ColorSupport::Extended;
        } else if term.contains("color") {
            return ColorSupport::Basic;
        }
    }

    // Default to basic color support
    ColorSupport::Basic
} 