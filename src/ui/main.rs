//! User Interface Module
//!
//! This module provides a terminal-based user interface for the mtr-ng network diagnostic tool.
//! It includes colorblind-friendly visualizations, sparkline graphs, interactive controls,
//! and support for various terminal color modes.

use crate::args::Column;
use crate::ui::events::EventHandler;
use crate::ui::state::UiState;
use crate::ui::visualization::{
    create_heatmap_spans, create_sparkline_spans, VisualizationMode,
};
use crate::ui::widgets;
use crate::utils;
use crate::{MtrSession, Result};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,

    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Cell, Clear, Paragraph, Row, Table},
    Frame, Terminal,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tracing::debug;

// ========================================
// Main UI Rendering
// ========================================

/// Detect the terminal's color support capabilities






// ========================================
// Table Components
// ========================================







// ========================================
// Main Rendering Function
// ========================================









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
        .map(|d| utils::time::duration_to_ms_u64(*d))
        .collect();

    let global_max_rtt = rtt_values.iter().max().copied().unwrap_or(1);
    let global_min_rtt = rtt_values.iter().min().copied().unwrap_or(1);

    // Status line (no borders)
    let status_line = widgets::create_status_text(session, ui_state);
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
        let hostname = widgets::format_hostname(session, hop, ui_state);
        let graph_width = widgets::calculate_graph_width(&chunks[1], &ui_state.columns);

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

        let cells = widgets::create_table_cells(
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

                let _alt_rtt = utils::time::duration_to_ms_f64(alt_path.last_rtt.unwrap_or_default());

                // Create cells for each column, focusing on key info
                let mut alt_cells = Vec::new();
                for column in &ui_state.columns {
                    match column {
                        Column::Hop => alt_cells.push(Cell::from("")),
                        Column::Host => alt_cells.push(Cell::from(alt_hostname.clone())),
                        Column::Loss => alt_cells.push(Cell::from("")), // Empty - percentage is now in hostname
                        Column::Sent => alt_cells.push(Cell::from("")),
                        Column::Last => {
                            if let Some(rtt) = alt_path.last_rtt {
                                let formatted = if utils::time::duration_to_us_f64(rtt) < 1000.0 {
                                    utils::time::format_duration_us(rtt)
                                } else {
                                    format!("{:.1}", utils::time::duration_to_ms_f64(rtt))
                                };
                                alt_cells.push(Cell::from(formatted));
                            } else {
                                alt_cells.push(Cell::from("???"));
                            }
                        },
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

    let constraints = widgets::create_column_constraints(&ui_state.columns);
    let table = Table::new(rows, &constraints).header(header);

    f.render_widget(table, chunks[1]);

    // Compact scale visualization
    let scale_widget = widgets::create_scale_widget(
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
        let (help_width, help_height) = utils::layout::calculate_popup_dimensions(
            area.width, area.height, 50, 12
        );
        let (help_x, help_y) = utils::layout::center_popup(
            area.width, area.height, help_width, help_height
        );

        let help_area = Rect {
            x: help_x,
            y: help_y,
            width: help_width,
            height: help_height,
        };

        // Clear the background and render help
        f.render_widget(Clear, help_area);
        f.render_widget(widgets::create_help_overlay(), help_area);
    }

    // Show column selector popup if enabled
    if ui_state.show_column_selector {
        let area = f.area();
        // Center the column selector popup - make it larger than help
        let preferred_height = (ui_state.column_selector_state.available_columns.len() + 8) as u16;
        let (popup_width, popup_height) = utils::layout::calculate_popup_dimensions(
            area.width, area.height, 60, preferred_height
        );
        let (popup_x, popup_y) = utils::layout::center_popup(
            area.width, area.height, popup_width, popup_height
        );

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        // Clear the background and render column selector
        f.render_widget(Clear, popup_area);
        f.render_widget(
            widgets::create_column_selector_popup(&ui_state.column_selector_state),
            popup_area,
        );
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

    let mut event_handler = EventHandler::new();

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

    // Create a channel for keyboard input events
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<crossterm::event::Event>();
    
    // Spawn a task to handle keyboard input asynchronously
    let input_handle = tokio::spawn(async move {
        loop {
            if let Ok(true) = crossterm::event::poll(Duration::from_millis(16)) {
                if let Ok(event) = crossterm::event::read() {
                    if input_tx.send(event).is_err() {
                        break; // Channel closed
                    }
                } else {
                    // Error reading input
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            } else {
                // No input available, yield briefly
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    });

    loop {
        // Pure event-driven: wait for data updates or keyboard input
        tokio::select! {
            // Wait for update notification from session (blocks until data arrives)
            update_result = update_rx.recv() => {
                if update_result.is_none() {
                    // Channel closed, session ended
                    break;
                }
                
                // Update UI immediately when new data arrives
                terminal.draw(|f| {
                    let session_guard = session_clone.lock().unwrap();
                    render_ui(f, &session_guard, &ui_state)
                })?;
            }
            
            // Handle keyboard input events immediately
            input_event = input_rx.recv() => {
                if let Some(Event::Key(key)) = input_event {
                    // Handle column selector popup inputs first
                    if ui_state.show_column_selector {
                        event_handler.handle_column_selector_input(
                            key.code,
                            key.modifiers,
                            &mut ui_state,
                        );
                    } else {
                        // Handle normal keyboard shortcuts
                        let should_continue = event_handler.handle_normal_input(
                            key.code,
                            &mut ui_state,
                            &session_clone,
                        );
                        if !should_continue {
                            break;
                        }
                    }
                    
                    // ALWAYS redraw UI immediately after keyboard input
                    terminal.draw(|f| {
                        let session_guard = session_clone.lock().unwrap();
                        render_ui(f, &session_guard, &ui_state)
                    })?;
                } else if input_event.is_none() {
                    // Input channel closed
                    break;
                }
            }
        }
    }

    input_handle.abort();
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
