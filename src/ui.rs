//! User Interface Module
//!
//! This module provides a terminal-based user interface for the mtr-ng network diagnostic tool.
//! It includes colorblind-friendly visualizations, sparkline graphs, interactive controls,
//! and support for various terminal color modes.

use crate::args::Column;

use crate::SparklineScale;
use crate::{HopStats, MtrSession, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,

    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
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
    pub show_help: bool,
    pub visualization_mode: VisualizationMode,
    pub show_hostnames: bool, // Toggle between hostnames and IP addresses
    pub show_column_selector: bool, // Show column selection popup
    pub column_selector_state: ColumnSelectorState, // State for column selector
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

// ========================================
// Column Selector State
// ========================================

#[derive(Debug, Clone)]
pub struct ColumnSelectorState {
    pub selected_index: usize,
    pub available_columns: Vec<(Column, bool)>, // (column, is_enabled)
}

impl ColumnSelectorState {
    pub fn new(enabled_columns: &[Column]) -> Self {
        let all_columns = Column::all();
        let available_columns = all_columns
            .into_iter()
            .map(|col| (col, enabled_columns.contains(&col)))
            .collect();

        Self {
            selected_index: 0,
            available_columns,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index < self.available_columns.len().saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some((_col, enabled)) = self.available_columns.get_mut(self.selected_index) {
            *enabled = !*enabled;
        }
    }

    pub fn move_selected_up(&mut self) {
        if self.selected_index > 0 {
            self.available_columns
                .swap(self.selected_index - 1, self.selected_index);
            self.selected_index -= 1;
        }
    }

    pub fn move_selected_down(&mut self) {
        if self.selected_index < self.available_columns.len().saturating_sub(1) {
            self.available_columns
                .swap(self.selected_index, self.selected_index + 1);
            self.selected_index += 1;
        }
    }

    pub fn get_enabled_columns(&self) -> Vec<Column> {
        self.available_columns
            .iter()
            .filter(|(_, enabled)| *enabled)
            .map(|(col, _)| *col)
            .collect()
    }
}

impl UiState {
    pub fn new(scale: SparklineScale, columns: Vec<Column>) -> Self {
        let column_selector_state = ColumnSelectorState::new(&columns);
        Self {
            current_sparkline_scale: scale,
            color_support: detect_color_support(),
            columns,
            current_column_index: 0,
            show_help: false,
            visualization_mode: VisualizationMode::Sparkline,
            show_hostnames: true, // Start with hostnames enabled by default
            show_column_selector: false,
            column_selector_state,
        }
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn toggle_column_selector(&mut self) {
        if self.show_column_selector {
            // Just close - changes were already applied immediately
        } else {
            // Reset selector state when opening
            self.column_selector_state = ColumnSelectorState::new(&self.columns);
        }
        self.show_column_selector = !self.show_column_selector;
    }

    // Immediate update methods for live preview
    pub fn toggle_selected_column_immediate(&mut self) {
        self.column_selector_state.toggle_selected();
        self.apply_column_changes_immediate();
    }

    pub fn move_selected_column_up_immediate(&mut self) {
        self.column_selector_state.move_selected_up();
        self.apply_column_changes_immediate();
    }

    pub fn move_selected_column_down_immediate(&mut self) {
        self.column_selector_state.move_selected_down();
        self.apply_column_changes_immediate();
    }

    fn apply_column_changes_immediate(&mut self) {
        self.columns = self.column_selector_state.get_enabled_columns();
        // Ensure at least one column remains
        if self.columns.is_empty() {
            self.columns.push(Column::Host);
            if let Some((_, enabled)) = self.column_selector_state
                .available_columns
                .iter_mut()
                .find(|(col, _)| matches!(col, Column::Host)) {
                *enabled = true;
            }
        }
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



/// Generate table cells for a hop
fn create_table_cells(
    hop: &HopStats,
    hostname: &str,
    sparkline_spans: &[Span<'static>],
    columns: &[Column],
) -> Vec<Cell<'static>> {
    columns
        .iter()
        .map(|column| {
            match column {
                Column::Hop => {
                    if hop.has_multiple_paths() {
                        Cell::from(format!("{:>1}*", hop.hop))
                    } else {
                        Cell::from(format!("{:>2}", hop.hop))
                    }
                }
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
                    if !sparkline_spans.is_empty() {
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
                        // Use 35% of available space when graph is present (increased for multi-path)
                        Constraint::Percentage(35)
                    } else {
                        Constraint::Min(20)
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
                Column::Graph => Constraint::Percentage(65), // Use 65% of available space (reduced to accommodate larger hostname column)
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

/// Create column selection popup
fn create_column_selector_popup(state: &ColumnSelectorState) -> Paragraph<'static> {
    let mut lines = vec![
        Line::from(vec![Span::styled(
            "Column Selection & Ordering",
            Style::default().fg(Color::Yellow),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(Color::Green)),
            Span::raw(" - Navigate  "),
            Span::styled("Space", Style::default().fg(Color::Green)),
            Span::raw(" - Toggle"),
        ]),
        Line::from(vec![
            Span::styled("←/→", Style::default().fg(Color::Green)),
            Span::raw(" or "),
            Span::styled("Shift+↑/↓", Style::default().fg(Color::Green)),
            Span::raw(" - Reorder columns"),
        ]),
        Line::from(""),
    ];

    for (i, (column, enabled)) in state.available_columns.iter().enumerate() {
        let column_name = match column {
            Column::Hop => "Hop Number",
            Column::Host => "Hostname/IP",
            Column::Loss => "Packet Loss %",
            Column::Sent => "Packets Sent",
            Column::Last => "Last RTT",
            Column::Avg => "Average RTT",
            Column::Ema => "EMA RTT",
            Column::Jitter => "Last Jitter",
            Column::JitterAvg => "Average Jitter",
            Column::Best => "Best RTT",
            Column::Worst => "Worst RTT",
            Column::Graph => "RTT Graph",
        };

        let checkbox = if *enabled { "☑" } else { "☐" };
        let is_selected = i == state.selected_index;

        let style = if is_selected {
            Style::default().fg(Color::Black).bg(Color::White)
        } else {
            Style::default()
        };

        let checkbox_style = if *enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Gray)
        };

        // Add position indicator (no cursor needed)
        let position_indicator = format!("{:2}.", i + 1);

        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", position_indicator),
                if is_selected {
                    Style::default().fg(Color::Yellow).bg(Color::White)
                } else {
                    Style::default().fg(Color::Gray)
                },
            ),
            Span::styled(
                format!(" {} ", checkbox),
                if is_selected {
                    checkbox_style.bg(Color::White)
                } else {
                    checkbox_style
                },
            ),
            Span::styled(column_name.to_string(), style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Green)),
        Span::raw(" - Close"),
    ]));

    // Add debug info showing current selection
    lines.push(Line::from(vec![Span::styled(
        format!(
            "Selection: {} of {}",
            state.selected_index + 1,
            state.available_columns.len()
        ),
        Style::default().fg(Color::Cyan),
    )]));

    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Column Settings")
                .title_alignment(Alignment::Center),
        )
        .alignment(Alignment::Left)
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
            Span::styled("o", Style::default().fg(Color::Green)),
            Span::raw("        - Open column selector"),
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

    let mut rows = Vec::new();

    // Determine how many hops to display based on discovery or organic growth
    let max_hops_to_display = if session.num_hosts > 0 {
        session.num_hosts
    } else {
        // Organic discovery: show hops up to the furthest one with data
        session.hops.iter()
            .enumerate()
            .rev()
            .find(|(_, hop)| hop.sent > 0 || hop.addr.is_some())
            .map(|(i, _)| i + 1)
            .unwrap_or(0)
            .max(8) // Show at least 8 hops to see progress
    };
    
    for hop in session.hops.iter().take(max_hops_to_display).filter(|hop| hop.sent > 0) {
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
        );

        rows.push(Row::new(cells));

        // Add alternate paths if multi-path is detected
        if hop.has_multiple_paths() {
            for alt_path in hop.get_alternate_paths() {
                let percentage = hop.get_path_percentage(alt_path);

                // Format hostname with proper length, including percentage
                let alt_hostname = if let Some(hostname) = &alt_path.hostname {
                    let full_name =
                        format!("  ↳ {} ({}) ({:.0}%)", hostname, alt_path.addr, percentage);
                    if full_name.len() > 50 {
                        format!(
                            "  ↳ {}...{} ({:.0}%)",
                            &hostname[..15],
                            alt_path.addr,
                            percentage
                        )
                    } else {
                        full_name
                    }
                } else {
                    format!("  ↳ {} ({:.0}%)", alt_path.addr, percentage)
                };

                let alt_rtt = alt_path.last_rtt.unwrap_or_default().as_secs_f64() * 1000.0;

                // Create cells for each column, focusing on key info
                let mut alt_cells = Vec::new();
                for column in &ui_state.columns {
                    match column {
                        Column::Hop => alt_cells.push(Cell::from("")),
                        Column::Host => alt_cells.push(Cell::from(alt_hostname.clone())),
                        Column::Loss => alt_cells.push(Cell::from("")), // Empty - percentage is now in hostname
                        Column::Sent => alt_cells.push(Cell::from("")),
                        Column::Last => alt_cells.push(Cell::from(format!("{:.1}", alt_rtt))),
                        Column::Avg => alt_cells.push(Cell::from("")),
                        Column::Ema => alt_cells.push(Cell::from("")),
                        Column::Best => alt_cells.push(Cell::from("")),
                        Column::Worst => alt_cells.push(Cell::from("")),
                        Column::Jitter => alt_cells.push(Cell::from("")),
                        Column::JitterAvg => alt_cells.push(Cell::from("")),
                        Column::Graph => alt_cells.push(Cell::from("")),
                    }
                }

                let alt_row = Row::new(alt_cells);
                rows.push(alt_row);
            }
        }
    }

    let constraints = create_column_constraints(&ui_state.columns);
    let table = Table::new(rows, &constraints).header(header);

    f.render_widget(table, chunks[1]);

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

    // Show column selector popup if enabled
    if ui_state.show_column_selector {
        let area = f.area();
        // Center the column selector popup - make it larger than help
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = (ui_state.column_selector_state.available_columns.len() + 8)
            .min(area.height.saturating_sub(4) as usize) as u16;
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Clear the background and render column selector
        f.render_widget(Clear, popup_area);
        f.render_widget(
            create_column_selector_popup(&ui_state.column_selector_state),
            popup_area,
        );
    }
}

fn format_hostname(session: &MtrSession, hop: &HopStats, ui_state: &UiState) -> String {
    let base_hostname = if session.args.numeric || !ui_state.show_hostnames {
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

    // Add primary path percentage if multi-path
    let hostname = if hop.has_multiple_paths() {
        if hop.addr.is_some() {
            let primary_percentage = hop.get_primary_path_percentage();
            format!("{} ({:.0}%)", base_hostname, primary_percentage)
        } else {
            base_hostname
        }
    } else {
        base_hostname
    };

    // With 20% width allocation, truncate longer hostnames appropriately
    const MAX_HOSTNAME_LEN: usize = 40; // Increased to accommodate percentage
    const TRUNCATED_LEN: usize = 37;

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

        // Remaining space for Host (35%) and Graph (65%) columns
        let remaining_width = total_width.saturating_sub(fixed_width);
        let graph_width = (remaining_width * 65) / 100;

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
                // Handle column selector popup inputs first
                if ui_state.show_column_selector {
                    match key.code {
                        KeyCode::Esc => {
                            // Close column selector
                            ui_state.show_column_selector = false;
                        }
                        KeyCode::Up => {
                            ui_state.column_selector_state.move_up();
                        }
                        KeyCode::Down => {
                            ui_state.column_selector_state.move_down();
                        }
                        KeyCode::Char(' ') => {
                            ui_state.toggle_selected_column_immediate();
                        }
                        KeyCode::Left => {
                            // Move selected column up in list
                            ui_state.move_selected_column_up_immediate();
                        }
                        KeyCode::Right => {
                            // Move selected column down in list
                            ui_state.move_selected_column_down_immediate();
                        }
                        _ => {
                            // Check for Shift+Up/Down for reordering (alternative to Left/Right)
                            if key.modifiers == crossterm::event::KeyModifiers::SHIFT {
                                match key.code {
                                    KeyCode::Up => {
                                        ui_state.move_selected_column_up_immediate();
                                    }
                                    KeyCode::Down => {
                                        ui_state.move_selected_column_down_immediate();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                } else {
                    // Handle normal keyboard shortcuts
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
                        KeyCode::Char('o') => ui_state.toggle_column_selector(),
                        KeyCode::Char('v') => ui_state.toggle_visualization_mode(),
                        KeyCode::Char('h') => ui_state.toggle_hostnames(),
                        KeyCode::Char('?') => ui_state.toggle_help(),
                        _ => {}
                    }
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
