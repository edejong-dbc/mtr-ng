//! User Interface Module
//!
//! This module provides a terminal-based user interface for the mtr-ng network diagnostic tool.
//! It includes colorblind-friendly visualizations, sparkline graphs, interactive controls,
//! and support for various terminal color modes.

use crate::args::Column;
use crate::sixel::SixelRenderer;
use crate::SparklineScale;
use crate::{HopStats, MtrSession, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Widget},
    Frame, Terminal,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tracing::debug;

// ========================================
// UI State Management
// ========================================

#[derive(Debug, Clone)]
pub struct UiState {
    pub current_sparkline_scale: SparklineScale,
    pub color_support: ColorSupport,
    pub columns: Vec<Column>,
    pub current_column_index: usize,
    pub sixel_renderer: SixelRenderer,
    pub show_help: bool,
    pub visualization_mode: VisualizationMode,
    pub show_hostnames: bool, // Toggle between hostnames and IP addresses
}

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

impl UiState {
    pub fn new(scale: SparklineScale, columns: Vec<Column>, enable_sixel: bool) -> Self {
        Self {
            current_sparkline_scale: scale,
            color_support: detect_color_support(),
            columns,
            current_column_index: 0,
            sixel_renderer: SixelRenderer::new(enable_sixel),
            show_help: false,
            visualization_mode: VisualizationMode::Sparkline,
            show_hostnames: true, // Start with hostnames enabled by default
        }
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn toggle_visualization_mode(&mut self) {
        self.visualization_mode = match self.visualization_mode {
            VisualizationMode::Sparkline => VisualizationMode::Heatmap,
            VisualizationMode::Heatmap => VisualizationMode::Sparkline,
        };
    }

    pub fn toggle_hostnames(&mut self) {
        self.show_hostnames = !self.show_hostnames;
    }

    pub fn toggle_sparkline_scale(&mut self) {
        self.current_sparkline_scale = match self.current_sparkline_scale {
            SparklineScale::Linear => SparklineScale::Logarithmic,
            SparklineScale::Logarithmic => SparklineScale::Linear,
        };
    }

    pub fn cycle_color_mode(&mut self) {
        self.color_support = match self.color_support {
            ColorSupport::None => ColorSupport::Basic,
            ColorSupport::Basic => ColorSupport::Extended,
            ColorSupport::Extended => ColorSupport::TrueColor,
            ColorSupport::TrueColor => ColorSupport::None,
        };
    }

    pub fn toggle_column(&mut self) {
        if !self.columns.is_empty() {
            self.current_column_index = (self.current_column_index + 1) % self.columns.len();
            let all_columns = Column::all();
            let removed_column = self.columns.remove(self.current_column_index);

            for col in &all_columns {
                if !self.columns.contains(col) && *col != removed_column {
                    self.columns.insert(self.current_column_index, *col);
                    break;
                }
            }

            if self.current_column_index >= self.columns.len() {
                self.current_column_index = 0;
            }
        }
    }

    pub fn add_column(&mut self, column: Column) {
        if !self.columns.contains(&column) {
            self.columns.push(column);
        }
    }

    pub fn remove_column(&mut self, column: Column) {
        if let Some(pos) = self.columns.iter().position(|&c| c == column) {
            self.columns.remove(pos);
            if self.current_column_index >= self.columns.len() && self.current_column_index > 0 {
                self.current_column_index = self.columns.len() - 1;
            }
        }
    }

    pub fn get_header(&self) -> String {
        let mut header = String::from("  ");
        for (i, column) in self.columns.iter().enumerate() {
            if i > 0 {
                header.push(' ');
            }
            match column {
                Column::Hop => {} // No header for hop number column (3 chars: "XX.")
                Column::Host => header.push_str(&format!("{:21}", column.header())), // 21 chars
                Column::Loss => header.push_str(&format!("{:>7}", column.header())), // 7 chars for "XX.X%"
                Column::Sent => header.push_str(&format!("{:>4}", column.header())), // 4 chars
                Column::Last | Column::Avg | Column::Ema | Column::Best | Column::Worst => {
                    header.push_str(&format!("{:>9}", column.header())); // 9 chars for "XXX.Xms"
                }
                Column::Jitter | Column::JitterAvg => {
                    header.push_str(&format!("{:>9}", column.header())); // 9 chars for "XXX.Xms"
                }
                Column::Graph => header.push_str(column.header()), // Variable width
            }
        }
        header
    }
}

/// Detect the terminal's color support capabilities
fn detect_color_support() -> ColorSupport {
    if let Ok(colorterm) = std::env::var("COLORTERM") {
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return ColorSupport::TrueColor;
        }
    }

    if let Ok(term) = std::env::var("TERM") {
        if term.contains("256") || term.contains("256color") {
            return ColorSupport::Extended;
        }
        if term.contains("color") || term.starts_with("screen") {
            return ColorSupport::Basic;
        }
    }

    ColorSupport::Basic
}

// ========================================
// Color Management
// ========================================

/// Color scheme functions for RTT visualization
mod colors {
    use super::{Color, ColorSupport};

    pub fn get_rtt_color(ratio: f64, color_support: ColorSupport) -> (char, Color) {
        let level = (ratio.clamp(0.0, 1.0) * 8.0) as usize;
        let char = ['▁', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][level.min(8)];

        let color = match color_support {
            ColorSupport::None => Color::White,
            ColorSupport::Basic => [
                Color::Green,
                Color::Green,
                Color::Cyan,
                Color::Yellow,
                Color::Yellow,
                Color::Magenta,
                Color::Magenta,
                Color::Red,
                Color::Red,
            ][level.min(8)],
            ColorSupport::Extended => [17, 21, 39, 75, 111, 179, 215, 208, 130]
                .get(level)
                .map(|&i| Color::Indexed(i))
                .unwrap_or(Color::Indexed(130)),
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
                let (r, g, b) = colors[level.min(8)];
                Color::Rgb(r, g, b)
            }
        };

        (char, color)
    }

    pub fn get_smooth_gradient_color(ratio: f64, color_support: ColorSupport) -> Color {
        let ratio = ratio.clamp(0.0, 1.0);

        match color_support {
            ColorSupport::None => Color::White,
            ColorSupport::Basic => [
                Color::Blue,
                Color::Cyan,
                Color::Yellow,
                Color::Magenta,
                Color::Red,
            ][(ratio * 4.0) as usize],
            ColorSupport::Extended => {
                let steps = [17, 21, 33, 39, 75, 111, 179, 215];
                let index = (ratio * (steps.len() - 1) as f64) as usize;
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
                Color::Rgb(r as u8, g as u8, b as u8)
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
// Sparkline Visualization
// ========================================

/// Generate colored sparkline spans for RTT visualization
fn create_sparkline_spans(
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
fn create_heatmap_spans(
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
                    Span::styled("·".to_string(), Style::default().fg(color))
                }
            }
        })
        .collect();

    if spans.len() < max_width {
        spans.push(Span::raw(" ".repeat(max_width - spans.len())));
    }

    spans
}

fn calculate_rtt_ratio(
    rtt_ms: u64,
    global_min: u64,
    global_max: u64,
    scale: SparklineScale,
) -> f64 {
    if global_min == global_max || rtt_ms == 0 {
        return 0.0;
    }

    match scale {
        SparklineScale::Linear => rtt_ms as f64 / global_max as f64,
        SparklineScale::Logarithmic => {
            let log_rtt = ((rtt_ms + 1) as f64).log10();
            let log_min = ((global_min + 1) as f64).log10();
            let log_max = ((global_max + 1) as f64).log10();
            (log_rtt - log_min) / (log_max - log_min)
        }
    }
}

// ========================================
// Table Components
// ========================================

/// Table with optional Sixel support
pub struct EnhancedTable<'a> {
    table: Table<'a>,
    sixel_renderer: &'a SixelRenderer,
    columns: &'a [Column],
}

impl<'a> EnhancedTable<'a> {
    pub fn new(table: Table<'a>, sixel_renderer: &'a SixelRenderer, columns: &'a [Column]) -> Self {
        Self {
            table,
            sixel_renderer,
            columns,
        }
    }
}

impl<'a> Widget for EnhancedTable<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.table.render(area, buf);

        if !self.sixel_renderer.enabled {
            return;
        }

        // Add Sixel graphics for Graph column if present
        if let Some(_graph_col_idx) = self
            .columns
            .iter()
            .position(|col| matches!(col, Column::Graph))
        {
            // Sixel rendering logic would go here
            // For now, skip detailed implementation since it's complex
        }
    }
}

/// Generate table cells for a hop
fn create_table_cells(
    hop: &HopStats,
    hostname: &str,
    sparkline_spans: &[Span<'static>],
    columns: &[Column],
    sixel_enabled: bool,
) -> Vec<Cell<'static>> {
    columns
        .iter()
        .map(|column| {
            match column {
                Column::Hop => Cell::from(format!("{:>2}", hop.hop)),
                Column::Host => Cell::from(hostname.to_owned()),
                Column::Loss => {
                    let loss_pct = hop.loss_percent;
                    let color = if loss_pct > 50.0 {
                        Color::Red
                    } else if loss_pct > 10.0 {
                        Color::Yellow
                    } else {
                        Color::Green
                    };
                    Cell::from(format!("{:>4.1}%", loss_pct)).style(Style::default().fg(color))
                }
                Column::Sent => Cell::from(format!("{:>3}", hop.sent)),
                Column::Last => {
                    if let Some(last_rtt) = hop.rtts.back() {
                        Cell::from(format!("{:>6.1}", last_rtt.as_secs_f64() * 1000.0))
                    } else {
                        Cell::from(format!("{:>6}", "???"))
                    }
                }
                Column::Avg => {
                    if let Some(avg_rtt) = hop.avg_rtt {
                        Cell::from(format!("{:>6.1}", avg_rtt.as_secs_f64() * 1000.0))
                    } else {
                        Cell::from(format!("{:>6}", "???"))
                    }
                }
                Column::Ema => {
                    if let Some(ema_rtt) = hop.ema_rtt {
                        Cell::from(format!("{:>6.1}", ema_rtt.as_secs_f64() * 1000.0))
                    } else {
                        Cell::from(format!("{:>6}", "???"))
                    }
                }
                Column::Jitter => {
                    if let Some(jitter) = hop.last_jitter {
                        Cell::from(format!("{:>6.1}", jitter.as_secs_f64() * 1000.0))
                    } else {
                        Cell::from(format!("{:>6}", "???"))
                    }
                }
                Column::JitterAvg => {
                    if let Some(jitter_avg) = hop.jitter_avg {
                        Cell::from(format!("{:>6.1}", jitter_avg.as_secs_f64() * 1000.0))
                    } else {
                        Cell::from(format!("{:>6}", "???"))
                    }
                }
                Column::Best => {
                    if let Some(best_rtt) = hop.best_rtt {
                        Cell::from(format!("{:>6.1}", best_rtt.as_secs_f64() * 1000.0))
                    } else {
                        Cell::from(format!("{:>6}", "???"))
                    }
                }
                Column::Worst => {
                    if let Some(worst_rtt) = hop.worst_rtt {
                        Cell::from(format!("{:>6.1}", worst_rtt.as_secs_f64() * 1000.0))
                    } else {
                        Cell::from(format!("{:>6}", "???"))
                    }
                }
                Column::Graph => {
                    if sixel_enabled {
                        Cell::from("") // Sixel will fill this
                    } else if !sparkline_spans.is_empty() {
                        Cell::from(Line::from(sparkline_spans.to_vec()))
                    } else {
                        Cell::from("")
                    }
                }
            }
        })
        .collect()
}

/// Generate column constraints with dynamic sizing for Host and Graph columns
fn create_column_constraints(columns: &[Column]) -> Vec<Constraint> {
    let has_graph = columns.iter().any(|col| matches!(col, Column::Graph));

    columns
        .iter()
        .map(|column| {
            match column {
                Column::Hop => Constraint::Length(3),
                Column::Host => {
                    if has_graph {
                        // Use 20% of available space when graph is present
                        Constraint::Percentage(20)
                    } else {
                        Constraint::Min(15)
                    }
                }
                Column::Loss => Constraint::Length(5),
                Column::Sent => Constraint::Length(3),
                Column::Last | Column::Avg | Column::Ema | Column::Best | Column::Worst => {
                    if has_graph {
                        Constraint::Length(6)
                    } else {
                        Constraint::Length(9)
                    }
                }
                Column::Jitter | Column::JitterAvg => {
                    if has_graph {
                        Constraint::Length(6)
                    } else {
                        Constraint::Length(9)
                    }
                }
                Column::Graph => Constraint::Percentage(80), // Use 80% of available space
            }
        })
        .collect()
}

// ========================================
// Widget Creation Functions
// ========================================

/// Create inline status text without borders
fn create_status_text(session: &MtrSession, ui_state: &UiState) -> Line<'static> {
    let total_sent: usize = session.hops.iter().map(|h| h.sent).sum();
    let total_received: usize = session.hops.iter().map(|h| h.received).sum();
    let overall_loss = if total_sent > 0 {
        ((total_sent - total_received) as f64 / total_sent as f64) * 100.0
    } else {
        0.0
    };

    let active_hops = session.hops.iter().filter(|h| h.sent > 0).count();
    let scale_name = match ui_state.current_sparkline_scale {
        SparklineScale::Linear => "Linear",
        SparklineScale::Logarithmic => "Log",
    };

    let viz_mode = match ui_state.visualization_mode {
        VisualizationMode::Sparkline => "Sparkline",
        VisualizationMode::Heatmap => "Heatmap",
    };

    let hostname_mode = if ui_state.show_hostnames {
        "Hostnames"
    } else {
        "IPs"
    };

    let main_text = format!(
        "mtr-ng: {} → {} | Hops: {} | Sent: {} | Loss: {:.1}% | Scale: {} | Mode: {} | Display: {}",
        session.target,
        session.target_addr,
        active_hops,
        total_sent,
        overall_loss,
        scale_name,
        viz_mode,
        hostname_mode
    );

    Line::from(vec![
        Span::raw(main_text),
        Span::raw(" | "),
        Span::styled("? for help", Style::default().fg(Color::Gray)),
    ])
}

/// Create help overlay with keyboard shortcuts
fn create_help_overlay() -> Paragraph<'static> {
    let help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Shortcuts",
            Style::default().fg(Color::Yellow),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Green)),
            Span::raw(" / "),
            Span::styled("ESC", Style::default().fg(Color::Green)),
            Span::raw("  - Quit application"),
        ]),
        Line::from(vec![
            Span::styled("r", Style::default().fg(Color::Green)),
            Span::raw("        - Reset statistics"),
        ]),
        Line::from(vec![
            Span::styled("s", Style::default().fg(Color::Green)),
            Span::raw("        - Toggle sparkline scale (Linear/Log)"),
        ]),
        Line::from(vec![
            Span::styled("c", Style::default().fg(Color::Green)),
            Span::raw("        - Cycle color modes"),
        ]),
        Line::from(vec![
            Span::styled("f", Style::default().fg(Color::Green)),
            Span::raw("        - Toggle column fields"),
        ]),
        Line::from(vec![
            Span::styled("v", Style::default().fg(Color::Green)),
            Span::raw("        - Toggle visualization (Sparkline/Heatmap)"),
        ]),
        Line::from(vec![
            Span::styled("h", Style::default().fg(Color::Green)),
            Span::raw("        - Toggle hostnames/IP addresses"),
        ]),
        Line::from(vec![
            Span::styled("?", Style::default().fg(Color::Green)),
            Span::raw("        - Toggle this help"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press ? again to close",
            Style::default().fg(Color::Cyan),
        )]),
    ];

    Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .title_alignment(Alignment::Center),
        )
        .alignment(Alignment::Left)
}

/// Create compact scale visualization with multiple x-axis labels
fn create_scale_widget(
    min_rtt: u64,
    max_rtt: u64,
    scale: SparklineScale,
    color_support: ColorSupport,
    width: usize,
) -> Paragraph<'static> {
    if min_rtt == max_rtt {
        return Paragraph::new("No RTT data");
    }

    let scale_width = (width / 2).clamp(40, 80);
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    let scale_spans: Vec<Span> = (0..scale_width)
        .map(|i| {
            let ratio = i as f64 / (scale_width - 1) as f64;
            let level = (ratio * 7.0) as usize;
            let char = chars[level.min(7)];
            let color = colors::get_smooth_gradient_color(ratio, color_support);
            Span::styled(char.to_string(), Style::default().fg(color))
        })
        .collect();

    let scale_name = match scale {
        SparklineScale::Linear => "Linear",
        SparklineScale::Logarithmic => "Log₁₀",
    };

    // Create x-axis labels - use 5 evenly spaced points
    let num_labels = 5;
    let mut label_info = Vec::new();

    for i in 0..num_labels {
        let ratio = i as f64 / (num_labels - 1) as f64;

        let value = match scale {
            SparklineScale::Linear => min_rtt + (ratio * (max_rtt - min_rtt) as f64) as u64,
            SparklineScale::Logarithmic => {
                let log_min = (min_rtt as f64 + 1.0).ln();
                let log_max = (max_rtt as f64 + 1.0).ln();
                let log_value = log_min + ratio * (log_max - log_min);
                (log_value.exp() - 1.0) as u64
            }
        };

        let label = if value < 1000 {
            format!("{}ms", value)
        } else {
            format!("{:.1}s", value as f64 / 1000.0)
        };

        // Calculate the center position for this label on the gradient
        let center_pos = (ratio * (scale_width - 1) as f64) as usize;

        label_info.push((label, center_pos));
    }

    // Build the label line with centered positioning
    let mut label_spans = Vec::new();
    let mut current_pos = 0;

    for (i, (label, center_pos)) in label_info.iter().enumerate() {
        // Calculate where this label should start to be centered at center_pos
        let label_len = label.len();
        let label_start = center_pos.saturating_sub(label_len / 2);

        // Add spacing to reach the label start position
        if label_start > current_pos {
            let padding = label_start - current_pos;
            label_spans.push(Span::raw(" ".repeat(padding)));
            current_pos += padding;
        }

        // Add the label
        label_spans.push(Span::raw(label.clone()));
        current_pos += label_len;

        // For the last label, add scale type if there's space
        if i == label_info.len() - 1 {
            let remaining_space = scale_width.saturating_sub(current_pos);
            if remaining_space > scale_name.len() + 4 {
                label_spans.push(Span::raw(
                    " ".repeat(remaining_space - scale_name.len() - 3),
                ));
                label_spans.push(Span::styled(
                    format!("({})", scale_name),
                    Style::default().fg(Color::Gray),
                ));
            }
        }
    }

    let scale_text = vec![Line::from(label_spans), Line::from(scale_spans)];

    Paragraph::new(scale_text)
}

// ========================================
// Main UI Rendering
// ========================================

/// Main UI rendering function - now much more compact
/// Renders the main UI layout with status, table, and scale components
///
/// This function creates a 3-section layout:
/// 1. Status line - Shows connection info, statistics, and current modes  
/// 2. Main table - Displays hop data with optional graph visualization
/// 3. Scale widget - Shows RTT scale with gradient and labeled axis
///
/// The function also handles the help overlay when toggled by the user.
pub fn render_ui(f: &mut Frame, session: &MtrSession, ui_state: &UiState) {
    let area = f.area();

    // Minimum size check
    if area.height < 10 || area.width < 50 {
        let fallback = Paragraph::new(format!(
            "Terminal too small: {}x{}\nMinimum: 50x10\nPress 'q' to quit",
            area.width, area.height
        ));
        f.render_widget(fallback, area);
        return;
    }

    // Compact layout - no margins, minimal spacing
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Status line
            Constraint::Min(5),    // Main table
            Constraint::Length(2), // Scale (compact)
        ])
        .split(area);

    // Get RTT range for scaling
    let rtt_values: Vec<u64> = session
        .hops
        .iter()
        .filter(|hop| hop.sent > 0)
        .flat_map(|hop| hop.rtts.iter())
        .map(|d| (d.as_secs_f64() * 1000.0) as u64)
        .collect();

    let global_max_rtt = rtt_values.iter().max().copied().unwrap_or(1);
    let global_min_rtt = rtt_values.iter().min().copied().unwrap_or(1);

    // Status line (no borders)
    let status_line = create_status_text(session, ui_state);
    let status = Paragraph::new(vec![status_line]);
    f.render_widget(status, chunks[0]);

    // Main table
    let header_cells = ui_state.columns.iter().map(|col| match col {
        Column::Loss
        | Column::Sent
        | Column::Last
        | Column::Avg
        | Column::Ema
        | Column::Jitter
        | Column::JitterAvg
        | Column::Best
        | Column::Worst => Cell::from(format!("{:>width$}", col.header(), width = col.width())),
        _ => Cell::from(col.header()),
    });

    let header = Row::new(header_cells).style(Style::default().fg(Color::Yellow));

    let rows = session.hops.iter().filter(|hop| hop.sent > 0).map(|hop| {
        let hostname = format_hostname(session, hop, ui_state);
        let graph_width = calculate_graph_width(&chunks[1], &ui_state.columns);

        let graph_spans = match ui_state.visualization_mode {
            VisualizationMode::Sparkline => create_sparkline_spans(
                hop,
                global_min_rtt,
                global_max_rtt,
                ui_state.current_sparkline_scale,
                ui_state.color_support,
                graph_width,
            ),
            VisualizationMode::Heatmap => create_heatmap_spans(
                hop,
                global_min_rtt,
                global_max_rtt,
                ui_state.current_sparkline_scale,
                ui_state.color_support,
                graph_width,
            ),
        };

        let cells = create_table_cells(
            hop,
            &hostname,
            &graph_spans,
            &ui_state.columns,
            ui_state.sixel_renderer.enabled,
        );

        Row::new(cells)
    });

    let constraints = create_column_constraints(&ui_state.columns);
    let table = Table::new(rows, &constraints).header(header);

    // Use enhanced table for Sixel support
    if ui_state.sixel_renderer.enabled {
        let enhanced_table = EnhancedTable::new(table, &ui_state.sixel_renderer, &ui_state.columns);
        f.render_widget(enhanced_table, chunks[1]);
    } else {
        f.render_widget(table, chunks[1]);
    }

    // Compact scale visualization
    let scale_widget = create_scale_widget(
        global_min_rtt,
        global_max_rtt,
        ui_state.current_sparkline_scale,
        ui_state.color_support,
        chunks[2].width as usize,
    );
    f.render_widget(scale_widget, chunks[2]);

    // Show help overlay if enabled
    if ui_state.show_help {
        let area = f.area();
        // Center the help overlay
        let help_width = 50.min(area.width.saturating_sub(4));
        let help_height = 12.min(area.height.saturating_sub(4));
        let help_x = (area.width.saturating_sub(help_width)) / 2;
        let help_y = (area.height.saturating_sub(help_height)) / 2;

        let help_area = Rect {
            x: help_x,
            y: help_y,
            width: help_width,
            height: help_height,
        };

        // Clear the background and render help
        f.render_widget(Clear, help_area);
        f.render_widget(create_help_overlay(), help_area);
    }
}

fn format_hostname(session: &MtrSession, hop: &HopStats, ui_state: &UiState) -> String {
    let hostname = if session.args.numeric || !ui_state.show_hostnames {
        // Show IP addresses when numeric mode or hostname toggle is off
        hop.addr
            .map(|a| a.to_string())
            .unwrap_or_else(|| "???".to_string())
    } else {
        // Show hostnames when available, fallback to IP
        hop.hostname.clone().unwrap_or_else(|| {
            hop.addr
                .map(|a| a.to_string())
                .unwrap_or_else(|| "???".to_string())
        })
    };

    // With 20% width allocation, truncate longer hostnames appropriately
    const MAX_HOSTNAME_LEN: usize = 35;
    const TRUNCATED_LEN: usize = 32;

    if hostname.len() > MAX_HOSTNAME_LEN {
        format!("{}...", &hostname[..TRUNCATED_LEN])
    } else {
        hostname
    }
}

fn calculate_graph_width(table_area: &Rect, columns: &[Column]) -> usize {
    if columns.contains(&Column::Graph) {
        // Calculate actual width available for graph column (80% of remaining space)
        let total_width = table_area.width.saturating_sub(4) as usize; // Account for borders

        // Calculate space used by fixed-width columns
        let fixed_width: usize = columns
            .iter()
            .map(|col| {
                match col {
                    Column::Hop => 3,
                    Column::Loss => 5,
                    Column::Sent => 3,
                    Column::Last | Column::Avg | Column::Ema | Column::Best | Column::Worst => 6,
                    Column::Jitter | Column::JitterAvg => 6,
                    Column::Host | Column::Graph => 0, // These use percentage-based sizing
                }
            })
            .sum();

        // Remaining space for Host (20%) and Graph (80%) columns
        let remaining_width = total_width.saturating_sub(fixed_width);
        let graph_width = (remaining_width * 80) / 100;

        // Ensure minimum usable width but no upper cap
        graph_width.max(20)
    } else {
        20
    }
}

// ========================================
// Interactive Event Loop
// ========================================

pub async fn run_interactive(session: MtrSession) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let session_arc = Arc::new(Mutex::new(session.clone()));
    let session_clone = Arc::clone(&session_arc);

    let mut ui_state = UiState::new(
        session.args.sparkline_scale,
        session.args.get_columns(),
        session.args.sixel,
    );

    let (update_tx, mut update_rx) = mpsc::unbounded_channel::<()>();

    {
        let mut session_guard = session_arc.lock().unwrap();
        let update_tx_for_callback = update_tx.clone();
        session_guard.set_update_callback(Arc::new(move || {
            let _ = update_tx_for_callback.send(());
        }));
    }

    let trace_handle = {
        let session_for_trace = Arc::clone(&session_clone);
        tokio::spawn(async move {
            if let Err(e) = MtrSession::run_trace_with_realtime_updates(session_for_trace).await {
                debug!("Real-time trace failed: {}", e);
            }
        })
    };

    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100);

    loop {
        let should_update = update_rx.try_recv().is_ok() || last_tick.elapsed() >= tick_rate;

        if should_update {
            // Lock session only during rendering to get live updates
            terminal.draw(|f| {
                let session_guard = session_clone.lock().unwrap();
                render_ui(f, &session_guard, &ui_state)
            })?;
            last_tick = Instant::now();
        }

        let timeout = Duration::from_millis(10);
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => {
                        let mut session_guard = session_clone.lock().unwrap();
                        for hop in &mut session_guard.hops {
                            *hop = HopStats::new(hop.hop);
                        }
                    }
                    KeyCode::Char('s') => ui_state.toggle_sparkline_scale(),
                    KeyCode::Char('c') => ui_state.cycle_color_mode(),
                    KeyCode::Char('f') => ui_state.toggle_column(),
                    KeyCode::Char('v') => ui_state.toggle_visualization_mode(),
                    KeyCode::Char('h') => ui_state.toggle_hostnames(),
                    KeyCode::Char('?') => ui_state.toggle_help(),
                    _ => {}
                }
            }
        }
    }

    trace_handle.abort();
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
