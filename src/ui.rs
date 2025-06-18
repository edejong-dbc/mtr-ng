use crate::args::Column;
use crate::session::NetworkEvent;
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
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, Paragraph, Table, Row, Cell},
    Frame, Terminal,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct UiState {
    pub current_sparkline_scale: SparklineScale,
    pub color_support: ColorSupport,
    pub columns: Vec<Column>,
    pub current_column_index: usize,
    pub sixel_renderer: SixelRenderer,
}

#[derive(Debug, Clone, Copy)]
pub enum ColorSupport {
    None,      // No color support
    Basic,     // 16 colors
    Extended,  // 256 colors
    TrueColor, // 24-bit RGB
}

impl UiState {
    pub fn new(scale: SparklineScale, columns: Vec<Column>, enable_sixel: bool) -> Self {
        Self {
            current_sparkline_scale: scale,
            color_support: detect_color_support(),
            columns,
            current_column_index: 0,
            sixel_renderer: SixelRenderer::new(enable_sixel),
        }
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

            // Remove the current column and cycle to next available
            let all_columns = Column::all();
            let removed_column = self.columns.remove(self.current_column_index);

            // Find next column not currently displayed
            for col in &all_columns {
                if !self.columns.contains(col) && *col != removed_column {
                    self.columns.insert(self.current_column_index, *col);
                    break;
                }
            }

            // Reset index if it's out of bounds
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

fn detect_color_support() -> ColorSupport {
    // Check environment variables for color support
    if let Ok(colorterm) = std::env::var("COLORTERM") {
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            tracing::debug!("Detected TrueColor support from COLORTERM={}", colorterm);
            return ColorSupport::TrueColor;
        }
    }

    if let Ok(term) = std::env::var("TERM") {
        if term.contains("256") || term.contains("256color") {
            tracing::debug!("Detected 256 color support from TERM={}", term);
            return ColorSupport::Extended;
        }
        if term.contains("color") || term == "xterm" || term.starts_with("screen") {
            tracing::debug!("Detected basic color support from TERM={}", term);
            return ColorSupport::Basic;
        }
    }

    // Default to basic color support for interactive terminals
    tracing::debug!("Using default basic color support");
    ColorSupport::Basic
}

fn generate_colored_sparkline(
    hop: &crate::HopStats,
    global_min_rtt: u64,
    global_max_rtt: u64,
    scale: SparklineScale,
    color_support: ColorSupport,
    sixel_renderer: &SixelRenderer,
    max_width: usize,
) -> Vec<Span<'static>> {
    if hop.sent == 0 {
        return vec![];
    }

    // If Sixel is enabled and we have data, try to generate Sixel sparkline
    if sixel_renderer.enabled && !hop.rtts.is_empty() {
        let rtt_data: Vec<f64> = hop
            .rtts
            .iter()
            .map(|rtt| rtt.as_secs_f64() * 1000.0)
            .collect(); // Convert to milliseconds
        if !rtt_data.is_empty() {
            let sixel_graph = sixel_renderer.generate_sparkline(&rtt_data, 24, 12); // Better size for visibility
            if !sixel_graph.is_empty() {
                return vec![Span::raw(sixel_graph)];
            }
        }
    }

    // Use the chronological packet history from HopStats
    let packet_spans: Vec<Span<'static>> = hop.packet_history
        .iter()
        .map(|outcome| {
            match outcome {
                crate::hop_stats::PacketOutcome::Received(rtt) => {
                    let rtt_ms = (rtt.as_secs_f64() * 1000.0) as u64;
                    let ratio = match scale {
                        SparklineScale::Linear => rtt_ms as f64 / global_max_rtt as f64,
                        SparklineScale::Logarithmic => {
                            if rtt_ms == 0 || global_max_rtt == 0 || global_min_rtt == global_max_rtt {
                                0.0
                            } else {
                                // Logarithmic scaling: (log10(rtt + 1) - log10(min + 1)) / (log10(max + 1) - log10(min + 1))
                                let log_rtt = ((rtt_ms + 1) as f64).log10();
                                let log_min = ((global_min_rtt + 1) as f64).log10();
                                let log_max = ((global_max_rtt + 1) as f64).log10();
                                (log_rtt - log_min) / (log_max - log_min)
                            }
                        }
                    };

                    let (char, color) = get_rtt_char_and_color(ratio, color_support);
                    Span::styled(char.to_string(), Style::default().fg(color))
                }
                crate::hop_stats::PacketOutcome::Lost => {
                    let color = get_lost_packet_color(color_support);
                    Span::styled("·".to_string(), Style::default().fg(color))
                }
                crate::hop_stats::PacketOutcome::Pending => {
                    let color = get_pending_packet_color(color_support);
                    Span::styled("?".to_string(), Style::default().fg(color))
                }
            }
        })
        .collect();

    // If the sparkline is longer than max_width, show only the most recent data (right side)
    if packet_spans.len() > max_width {
        let skip_count = packet_spans.len() - max_width;
        packet_spans.into_iter().skip(skip_count).collect()
    } else {
        packet_spans
    }
}

fn get_rtt_char_and_color(ratio: f64, color_support: ColorSupport) -> (char, Color) {
    let level = (ratio * 8.0) as usize;
    let char = match level {
        0 => '▁', // Always show at least minimal bar instead of space
        1 => '▁',
        2 => '▂',
        3 => '▃',
        4 => '▄',
        5 => '▅',
        6 => '▆',
        7 => '▇',
        _ => '█',
    };

    // Colorblind-friendly color scheme based on RTT level
    let color = match color_support {
        ColorSupport::None => Color::White,
        ColorSupport::Basic => {
            // Use basic 16 colors - green to red spectrum that works for colorblind users
            match level {
                0..=1 => Color::Green,   // Fast - green
                2..=3 => Color::Cyan,    // Good - cyan
                4..=5 => Color::Yellow,  // Medium - yellow
                6..=7 => Color::Magenta, // Slow - magenta
                _ => Color::Red,         // Very slow - red
            }
        }
        ColorSupport::Extended => {
            // Use 256-color palette for smoother gradation
            // Using colorblind-friendly blues to oranges/reds
            match level {
                0 => Color::Indexed(22),  // Dark green
                1 => Color::Indexed(28),  // Green
                2 => Color::Indexed(34),  // Light green
                3 => Color::Indexed(40),  // Green-cyan
                4 => Color::Indexed(220), // Yellow
                5 => Color::Indexed(214), // Orange
                6 => Color::Indexed(208), // Dark orange
                7 => Color::Indexed(196), // Red
                _ => Color::Indexed(160), // Dark red
            }
        }
        ColorSupport::TrueColor => {
            // Use RGB colors for finest gradation - colorblind safe palette
            match level {
                0 => Color::Rgb(0, 100, 0),   // Dark green
                1 => Color::Rgb(0, 150, 0),   // Green
                2 => Color::Rgb(100, 200, 0), // Yellow-green
                3 => Color::Rgb(200, 200, 0), // Yellow
                4 => Color::Rgb(255, 150, 0), // Orange
                5 => Color::Rgb(255, 100, 0), // Dark orange
                6 => Color::Rgb(255, 50, 0),  // Red-orange
                7 => Color::Rgb(200, 0, 0),   // Red
                _ => Color::Rgb(150, 0, 0),   // Dark red
            }
        }
    };

    (char, color)
}

fn get_lost_packet_color(color_support: ColorSupport) -> Color {
    // Use a distinct color for lost packets that's visible to colorblind users
    match color_support {
        ColorSupport::None => Color::White,
        ColorSupport::Basic => Color::Red,
        ColorSupport::Extended => Color::Indexed(196), // Bright red
        ColorSupport::TrueColor => Color::Rgb(255, 0, 0), // Pure red
    }
}

fn get_pending_packet_color(color_support: ColorSupport) -> Color {
    // Use blue/purple for pending packets - distinct from RTT colors
    match color_support {
        ColorSupport::None => Color::White,
        ColorSupport::Basic => Color::Blue,
        ColorSupport::Extended => Color::Indexed(27), // Blue
        ColorSupport::TrueColor => Color::Rgb(100, 100, 255), // Light blue
    }
}


fn generate_table_cells(
    hop: &crate::HopStats,
    hostname: &str,
    loss_color: Color,
    sparkline_spans: &[Span<'static>],
    columns: &[Column],
) -> Vec<Cell<'static>> {
    let mut cells = Vec::new();
    
    for column in columns {
        match column {
            Column::Hop => {
                cells.push(Cell::from(format!("{:2}.", hop.hop)));
            }
            Column::Host => {
                cells.push(Cell::from(hostname.to_string()));
            }
            Column::Loss => {
                let loss_text = if hop.sent > 0 {
                    format!("{:5.1}%", hop.loss_percent)
                } else {
                    "    -".to_string()
                };
                cells.push(Cell::from(loss_text).style(Style::default().fg(loss_color)));
            }
            Column::Sent => {
                cells.push(Cell::from(format!("{:4}", hop.sent)));
            }
            Column::Last => {
                let last_text = if let Some(last_rtt) = hop.last_rtt {
                    format!("{:6.1}ms", last_rtt.as_secs_f64() * 1000.0)
                } else {
                    "   -".to_string()
                };
                cells.push(Cell::from(last_text));
            }
            Column::Avg => {
                let avg_text = if let Some(avg_rtt) = hop.avg_rtt {
                    format!("{:6.1}ms", avg_rtt.as_secs_f64() * 1000.0)
                } else {
                    "   -".to_string()
                };
                cells.push(Cell::from(avg_text));
            }
            Column::Ema => {
                let ema_text = if let Some(ema_rtt) = hop.ema_rtt {
                    format!("{:6.1}ms", ema_rtt.as_secs_f64() * 1000.0)
                } else {
                    "   -".to_string()
                };
                cells.push(Cell::from(ema_text));
            }
            Column::Best => {
                let best_text = if let Some(best_rtt) = hop.best_rtt {
                    format!("{:6.1}ms", best_rtt.as_secs_f64() * 1000.0)
                } else {
                    "   -".to_string()
                };
                cells.push(Cell::from(best_text));
            }
            Column::Worst => {
                let worst_text = if let Some(worst_rtt) = hop.worst_rtt {
                    format!("{:6.1}ms", worst_rtt.as_secs_f64() * 1000.0)
                } else {
                    "   -".to_string()
                };
                cells.push(Cell::from(worst_text));
            }
            Column::Jitter => {
                let jitter_text = if let Some(jitter) = hop.last_jitter {
                    format!("{:6.1}ms", jitter.as_secs_f64() * 1000.0)
                } else {
                    "   -".to_string()
                };
                cells.push(Cell::from(jitter_text));
            }
            Column::JitterAvg => {
                let jitter_avg_text = if let Some(jitter_avg) = hop.jitter_avg {
                    format!("{:6.1}ms", jitter_avg.as_secs_f64() * 1000.0)
                } else {
                    "   -".to_string()
                };
                cells.push(Cell::from(jitter_avg_text));
            }
            Column::Graph => {
                // Create a cell with the sparkline spans
                if !sparkline_spans.is_empty() {
                    cells.push(Cell::from(Line::from(sparkline_spans.to_vec())));
                } else {
                    cells.push(Cell::from(""));
                }
            }
        }
    }
    
    cells
}

fn generate_column_constraints(columns: &[Column]) -> Vec<Constraint> {
    // Check if Graph column is present
    let has_graph = columns.iter().any(|col| matches!(col, Column::Graph));
    
    columns.iter().map(|column| {
        match column {
            Column::Hop => Constraint::Length(3),     // "XX."
            Column::Host => Constraint::Min(15),      // Host names
            Column::Loss => Constraint::Length(7),    // "100.0%"
            Column::Sent => Constraint::Length(4),    // Number
            Column::Last | Column::Avg | Column::Ema | Column::Best | Column::Worst => {
                Constraint::Length(9) // "XXX.Xms"
            }
            Column::Jitter | Column::JitterAvg => Constraint::Length(9), // "XXX.Xms"
            Column::Graph => {
                // Graph column should fill remaining space
                if has_graph {
                    Constraint::Percentage(100) // Take all remaining space
                } else {
                    Constraint::Min(20)
                }
            }
        }
    }).collect()
}

fn generate_scale_visualization(
    global_min_rtt: u64,
    global_max_rtt: u64,
    scale: SparklineScale,
    color_support: ColorSupport,
    width: usize,
) -> Paragraph<'static> {
    if global_min_rtt == global_max_rtt {
        return Paragraph::new("No RTT data available")
            .block(Block::default().borders(Borders::ALL).title("Sparkline Scale"));
    }

    let scale_name = match scale {
        SparklineScale::Linear => "Linear",
        SparklineScale::Logarithmic => "Log₁₀",
    };

    // Generate a visual scale with sparkline characters
    let scale_chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let scale_width = (width / 2).min(40).max(10); // Use half width, between 10-40 chars
    
    let mut scale_spans = Vec::new();
    
    // Generate the visual scale bar
    for i in 0..scale_width {
        let ratio = i as f64 / (scale_width - 1) as f64;
        let level = (ratio * 8.0) as usize;
        let char = scale_chars.get(level).unwrap_or(&'█');
        let (_, color) = get_rtt_char_and_color(ratio, color_support);
        scale_spans.push(Span::styled(char.to_string(), Style::default().fg(color)));
    }
    
    // Create the scale text with range information
    let scale_text = vec![
        Line::from(vec![
            Span::raw(format!("{} Scale: ", scale_name)),
            Span::raw(format!("{}ms", global_min_rtt)),
            Span::raw(" "),
        ]),
        Line::from(scale_spans),
        Line::from(vec![
            Span::raw(" ".repeat(scale_name.len() + 8)), // Align with "Scale: "
            Span::raw(" ".repeat(scale_width.saturating_sub(format!("{}ms", global_max_rtt).len()))),
            Span::raw(format!("{}ms", global_max_rtt)),
        ]),
    ];

    Paragraph::new(scale_text)
        .block(Block::default().borders(Borders::ALL).title("Sparkline Scale"))
}

pub fn render_ui(f: &mut Frame, session: &MtrSession, ui_state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(8),    // Main table
            Constraint::Length(3), // Scale visualization
            Constraint::Length(5), // RTT graph
            Constraint::Length(3), // Status bar
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(format!(
        "mtr-ng: {} → {} ({})",
        "localhost", session.target, session.target_addr
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Network Trace"),
    );
    f.render_widget(header, chunks[0]);

    // Calculate global min and max RTT for sparkline scaling across all hops
    let rtt_values: Vec<u64> = session
        .hops
        .iter()
        .filter(|hop| hop.sent > 0)
        .flat_map(|hop| hop.rtts.iter())
        .map(|d| (d.as_secs_f64() * 1000.0) as u64)
        .collect();
    
    let global_max_rtt = rtt_values.iter().max().copied().unwrap_or(1);
    let global_min_rtt = rtt_values.iter().min().copied().unwrap_or(1);

    // Main statistics table with right-aligned numeric headers
    let header_cells = ui_state.columns.iter().map(|col| {
        match col {
            // Numeric columns should be right-aligned
            Column::Loss | Column::Sent | Column::Last | Column::Avg | Column::Ema | 
            Column::Jitter | Column::JitterAvg | Column::Best | Column::Worst => {
                Cell::from(format!("{:>width$}", col.header(), width = col.width()))
            }
            // Text columns stay left-aligned
            _ => Cell::from(col.header())
        }
    });
    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Yellow))
        .height(1);

    let rows = session
        .hops
        .iter()
        .filter(|hop| hop.sent > 0)
        .map(|hop| {
            let loss_color = if hop.loss_percent > 50.0 {
                Color::Red
            } else if hop.loss_percent > 10.0 {
                Color::Yellow
            } else {
                Color::Green
            };

            let hostname = if session.args.numeric {
                hop.addr
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "???".to_string())
            } else {
                hop.hostname.clone().unwrap_or_else(|| {
                    hop.addr
                        .map(|a| a.to_string())
                        .unwrap_or_else(|| "???".to_string())
                })
            };

            // Truncate hostname to fit in column (20 chars max)
            let hostname = if hostname.len() > 20 {
                format!("{}...", &hostname[..17])
            } else {
                hostname
            };

            // Calculate available width for Graph column
            let graph_width = if ui_state.columns.contains(&Column::Graph) {
                // Calculate used width by other columns
                let used_width: usize = ui_state.columns.iter()
                    .filter(|col| !matches!(col, Column::Graph))
                    .map(|col| col.width() + 1) // +1 for spacing
                    .sum();
                // Use remaining width, minimum 20 chars
                let table_width = chunks[1].width.saturating_sub(4) as usize; // -4 for borders
                table_width.saturating_sub(used_width).max(20)
            } else {
                20 // Default fallback
            };

            // Generate colored sparkline for RTT history including lost packets
            let sparkline_spans = generate_colored_sparkline(
                hop,
                global_min_rtt,
                global_max_rtt,
                ui_state.current_sparkline_scale,
                ui_state.color_support,
                &ui_state.sixel_renderer,
                graph_width,
            );

            // Generate table cells based on selected columns
            let cells = generate_table_cells(
                hop,
                &hostname,
                loss_color,
                &sparkline_spans,
                &ui_state.columns,
            );

            Row::new(cells).height(1)
        });

    // Generate column constraints based on selected columns
    let constraints = generate_column_constraints(&ui_state.columns);

    let table = Table::new(rows)
        .header(header)
        .widths(&constraints)
        .block(Block::default().borders(Borders::ALL).title("Network Trace"))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Gray));
    
    f.render_widget(table, chunks[1]);

    // Scale visualization
    let scale_widget = generate_scale_visualization(
        global_min_rtt,
        global_max_rtt,
        ui_state.current_sparkline_scale,
        ui_state.color_support,
        chunks[2].width.saturating_sub(4) as usize, // Available width minus borders
    );
    f.render_widget(scale_widget, chunks[2]);

    // RTT Graph for the target host
    if let Some(target_hop) = session.hops.iter().find(|h| h.received > 0) {
        let data: Vec<(f64, f64)> = target_hop
            .rtts
            .iter()
            .enumerate()
            .map(|(i, rtt)| (i as f64, rtt.as_secs_f64() * 1000.0))
            .collect();

        if !data.is_empty() {
            let max_rtt = data.iter().map(|(_, rtt)| *rtt).fold(0.0, f64::max);
            let min_rtt = data
                .iter()
                .map(|(_, rtt)| *rtt)
                .fold(f64::INFINITY, f64::min);

            let datasets = vec![Dataset::default()
                .name("RTT")
                .marker(symbols::Marker::Braille)
                .style(Style::default().fg(Color::Green))
                .data(&data)];

            let chart = Chart::new(datasets)
                .block(Block::default().title("RTT History").borders(Borders::ALL))
                .x_axis(
                    Axis::default()
                        .title("Samples")
                        .style(Style::default().fg(Color::Gray))
                        .bounds([0.0, data.len() as f64]),
                )
                .y_axis(
                    Axis::default()
                        .title("RTT (ms)")
                        .style(Style::default().fg(Color::Gray))
                        .bounds([min_rtt * 0.9, max_rtt * 1.1]),
                );
            f.render_widget(chart, chunks[3]);
        }
    }

    // Status bar
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

    let color_name = match ui_state.color_support {
        ColorSupport::None => "No Color",
        ColorSupport::Basic => "16 Colors",
        ColorSupport::Extended => "256 Colors",
        ColorSupport::TrueColor => "RGB Colors",
    };

    let status_text = format!(
        "Active Hops: {} | Total Sent: {} | Total Received: {} | Overall Loss: {:.1}% | Sparkline: {} | Colors: {} | Keys: 'q'=quit, 'r'=reset, 's'=scale, 'c'=colors, 'f'=fields",
        active_hops, total_sent, total_received, overall_loss, scale_name, color_name
    );

    let status_color = if overall_loss > 50.0 {
        Color::Red
    } else if overall_loss > 10.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(status_color))
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, chunks[4]);
}

pub async fn run_interactive(session: MtrSession) -> Result<()> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Shared state for the UI and trace runner
    let session_arc = Arc::new(Mutex::new(session.clone()));
    let session_clone = Arc::clone(&session_arc);

    // Create UI state with initial sparkline scale and columns from args
    let mut ui_state = UiState::new(
        session.args.sparkline_scale,
        session.args.get_columns(),
        session.args.sixel,
    );

    // Create update notification channel for real-time updates
    let (update_tx, mut update_rx) = mpsc::unbounded_channel::<()>();

    // Set up real-time callback on the shared session
    {
        let mut session_guard = session_arc.lock().unwrap();
        let update_tx_for_callback = update_tx.clone();
        session_guard.set_update_callback(Arc::new(move || {
            let _ = update_tx_for_callback.send(());
        }));
    }

    // Start the MTR algorithm in a background task with proper real-time updates
    let trace_handle = {
        let session_for_trace = Arc::clone(&session_clone);

        tokio::spawn(async move {
            // Run the real-time MTR algorithm that triggers UI updates on each ping response
            if let Err(e) = MtrSession::run_trace_with_realtime_updates(session_for_trace).await {
                debug!("Real-time trace failed: {}", e);
            }
        })
    };

    // Main UI loop with immediate updates
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100); // Fallback refresh rate

    loop {
        // Check for real-time update notifications or fallback timer
        let should_update = update_rx.try_recv().is_ok() || last_tick.elapsed() >= tick_rate;

        if should_update {
            // Create a snapshot of session data and release lock immediately
            let session_snapshot = {
                let session_guard = session_clone.lock().unwrap();
                session_guard.clone()
            }; // Lock released here!

            // Render using the snapshot (no lock held during UI rendering)
            terminal.draw(|f| render_ui(f, &session_snapshot, &ui_state))?;
            last_tick = Instant::now();
        }

        // Handle keyboard input with short timeout
        let timeout = Duration::from_millis(10);
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => {
                        // Reset statistics
                        let mut session_guard = session_clone.lock().unwrap();
                        for hop in &mut session_guard.hops {
                            *hop = HopStats::new(hop.hop);
                        }
                    }
                    KeyCode::Char('s') => {
                        // Toggle sparkline scale
                        ui_state.toggle_sparkline_scale();
                    }
                    KeyCode::Char('c') => {
                        // Cycle color mode
                        ui_state.cycle_color_mode();
                    }
                    KeyCode::Char('f') => {
                        // Toggle column visibility
                        ui_state.toggle_column();
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup
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

// Channel-based UI that receives network updates without lock contention
pub async fn run_interactive_with_channels(mut session: MtrSession) -> Result<()> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create UI state with initial sparkline scale and columns from args
    let mut ui_state = UiState::new(
        session.args.sparkline_scale,
        session.args.get_columns(),
        session.args.sixel,
    );

    // Create channel for receiving network updates
    let (event_sender, mut event_receiver) = mpsc::unbounded_channel::<NetworkEvent>();

    // Start the network trace in a background task
    let trace_handle = {
        let session_clone = session.clone();
        tokio::spawn(async move {
            // For now, use the existing real-time trace but send updates via channel
            // TODO: Implement proper channel-based network trace
            if let Err(e) = run_network_trace_with_events(session_clone, event_sender).await {
                debug!("Network trace failed: {}", e);
            }
        })
    };

    // Main UI loop - processes network events and renders
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100);

    loop {
        // Process all available network events (non-blocking)
        while let Ok(event) = event_receiver.try_recv() {
            match event {
                NetworkEvent::RTTUpdate(update) => {
                    // Update our local session state
                    if let Some(hop) = session.hops.get_mut(update.hop) {
                        hop.add_rtt(update.rtt);
                        if hop.addr.is_none() {
                            hop.addr = Some(update.addr);
                        }
                    }
                }
                NetworkEvent::HopTimeout { hop, sent_count: _ } => {
                    if let Some(hop_stats) = session.hops.get_mut(hop) {
                        hop_stats.add_timeout();
                    }
                }
                NetworkEvent::TargetReached { hop: _ } => {
                    // Handle target reached
                }
                NetworkEvent::RoundComplete { round: _ } => {
                    // Handle round completion
                }
            }
        }

        // Render UI (no locks needed - we own the session state)
        let should_update = last_tick.elapsed() >= tick_rate;
        if should_update {
            terminal.draw(|f| render_ui(f, &session, &ui_state))?;
            last_tick = Instant::now();
        }

        // Handle keyboard input
        let timeout = Duration::from_millis(10);
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => {
                        // Reset statistics
                        for hop in &mut session.hops {
                            *hop = HopStats::new(hop.hop);
                        }
                    }
                    KeyCode::Char('s') => {
                        // Toggle sparkline scale
                        ui_state.toggle_sparkline_scale();
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup
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

// Channel-based network trace that sends events instead of using shared state
async fn run_network_trace_with_events(
    session: MtrSession,
    event_sender: mpsc::UnboundedSender<NetworkEvent>,
) -> Result<()> {
    use std::sync::{Arc, Mutex};

    // Create a modified session that sends events via channel
    let session_arc = Arc::new(Mutex::new(session));

    // Set up a callback that sends channel events when RTT updates arrive
    {
        let mut session_guard = session_arc.lock().unwrap();
        let sender_clone = event_sender.clone();
        session_guard.set_update_callback(Arc::new(move || {
            // This callback is triggered, but we need to get the actual RTT data
            // For now, just trigger UI updates - the data will be read from the mutex
            // This is a hybrid approach while we transition to full channel architecture
            let _ = sender_clone.send(NetworkEvent::RoundComplete { round: 0 });
        }));
    }

    // Run the existing mutex-based trace
    MtrSession::run_trace_with_realtime_updates(session_arc).await
}
