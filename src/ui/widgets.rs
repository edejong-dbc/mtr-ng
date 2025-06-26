//! UI Widget Creation Functions
//!
//! This module contains functions for creating various UI widgets and components
//! used in the mtr-ng terminal interface, including tables, popups, status text,
//! and layout calculations.

use crate::args::Column;
use crate::ui::visualization::{ColorSupport, VisualizationMode};
use crate::utils;
use crate::{HopStats, MtrSession, SparklineScale};
use ratatui::{
    layout::{Alignment, Constraint},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph},
};

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

// ========================================
// Table Cell and Layout Functions
// ========================================

/// Create table cells for a hop row
pub fn create_table_cells(
    hop: &HopStats,
    hostname: &str,
    sparkline_spans: &[Span<'static>],
    columns: &[Column],
) -> Vec<Cell<'static>> {
    columns
        .iter()
        .map(|column| {
            let cell_content = match column {
                Column::Hop => hop.hop.to_string(),
                Column::Host => hostname.to_string(),
                Column::Loss => {
                    if hop.sent > 0 {
                        format!("{:.1}%", hop.loss_percent)
                    } else {
                        "0.0%".to_string()
                    }
                }
                Column::Sent => hop.sent.to_string(),
                Column::Last => {
                    if let Some(rtt) = hop.last_rtt {
                        // Use microsecond precision for very fast connections (< 1ms)
                        if utils::time::duration_to_us_f64(rtt) < 1000.0 {
                            utils::time::format_duration_us(rtt)
                        } else {
                            utils::time::format_duration_ms(rtt)
                        }
                    } else {
                        "???".to_string()
                    }
                },
                Column::Avg => utils::time::format_optional_duration_ms(hop.avg_rtt),
                Column::Ema => utils::time::format_optional_duration_ms(hop.ema_rtt),
                Column::Jitter => utils::time::format_optional_duration_ms(hop.last_jitter),
                Column::JitterAvg => utils::time::format_optional_duration_ms(hop.jitter_avg),
                Column::Best => utils::time::format_optional_duration_ms(hop.best_rtt),
                Column::Worst => utils::time::format_optional_duration_ms(hop.worst_rtt),
                Column::Graph => {
                    return Cell::from(Line::from(sparkline_spans.to_vec()));
                }
            };

            Cell::from(cell_content)
        })
        .collect()
}

/// Create column layout constraints
pub fn create_column_constraints(columns: &[Column]) -> Vec<Constraint> {
    columns
        .iter()
        .map(|column| {
            match column {
                Column::Hop => Constraint::Length(3),
                Column::Host => Constraint::Percentage(20), // Increased to 20% for better readability
                Column::Loss => Constraint::Length(5),
                Column::Sent => Constraint::Length(3),
                Column::Last | Column::Avg | Column::Ema | Column::Best | Column::Worst => {
                    if columns.contains(&Column::Graph) {
                        Constraint::Length(6)
                    } else {
                        Constraint::Length(9)
                    }
                }
                Column::Jitter | Column::JitterAvg => {
                    if columns.contains(&Column::Graph) {
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
pub fn create_status_text(session: &MtrSession, ui_state: &super::UiState) -> Line<'static> {
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
pub fn create_column_selector_popup(state: &ColumnSelectorState) -> Paragraph<'static> {
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
pub fn create_help_overlay() -> Paragraph<'static> {
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
            Span::raw("        - Toggle visualization mode"),
        ]),
        Line::from(vec![
            Span::styled("h", Style::default().fg(Color::Green)),
            Span::raw("        - Toggle hostname display"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Column Selector (when open):",
            Style::default().fg(Color::Cyan),
        )]),
        Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(Color::Green)),
            Span::raw("      - Navigate columns"),
        ]),
        Line::from(vec![
            Span::styled("Space", Style::default().fg(Color::Green)),
            Span::raw("     - Toggle column visibility"),
        ]),
        Line::from(vec![
            Span::styled("←/→", Style::default().fg(Color::Green)),
            Span::raw("      - Reorder columns"),
        ]),
        Line::from(vec![
            Span::styled("Shift+↑/↓", Style::default().fg(Color::Green)),
            Span::raw(" - Alternative column reordering"),
        ]),
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

/// Create RTT scale visualization widget
pub fn create_scale_widget(
    min_rtt: u64,
    max_rtt: u64,
    scale: SparklineScale,
    color_support: ColorSupport,
    width: usize,
) -> Paragraph<'static> {
    if min_rtt == max_rtt {
        return Paragraph::new("No RTT data available");
    }

            let scale_width = utils::layout::constrain_width(width as u16, 20, 60) as usize;

    // Create gradient visualization using the same color logic as sparklines
    let gradient_spans: Vec<Span> = (0..scale_width)
        .map(|i| {
            let ratio = i as f64 / (scale_width - 1) as f64;
            let color = super::visualization::colors::get_smooth_gradient_color(ratio, color_support);
            Span::styled("█".to_string(), Style::default().fg(color))
        })
        .collect();



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
        label_spans.push(Span::styled(
            label.clone(),
            Style::default().fg(Color::White),
        ));
        current_pos += label_len;

        // Add spacing between labels (except for the last one)
        if i < label_info.len() - 1 {
            let next_label_start = label_info[i + 1].1.saturating_sub(label_info[i + 1].0.len() / 2);
            if next_label_start > current_pos {
                let spacing = next_label_start - current_pos;
                label_spans.push(Span::raw(" ".repeat(spacing)));
                current_pos += spacing;
            }
        }
    }

    let content = vec![
        Line::from(gradient_spans),
        Line::from(label_spans),
    ];

    Paragraph::new(content)
}

// ========================================
// Utility Functions
// ========================================

/// Format hostname for display with length constraints
pub fn format_hostname(session: &MtrSession, hop: &HopStats, ui_state: &super::UiState) -> String {
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

/// Calculate available width for graph column
pub fn calculate_graph_width(table_area: &ratatui::layout::Rect, columns: &[Column]) -> usize {
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
        utils::math::max_with_minimum(graph_width, 20)
    } else {
        20
    }
} 