use crate::{MtrSession, HopStats, Result};
use crate::session::{NetworkEvent, RTTUpdate};
use crate::SparklineScale;
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
    widgets::{
        Axis, Block, Borders, Chart, Dataset, List, ListItem, Paragraph,
    },
    Frame, Terminal,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tracing::debug;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct UiState {
    pub current_sparkline_scale: SparklineScale,
    pub color_support: ColorSupport,
}

#[derive(Debug, Clone, Copy)]
pub enum ColorSupport {
    None,      // No color support
    Basic,     // 16 colors
    Extended,  // 256 colors
    TrueColor, // 24-bit RGB
}

impl UiState {
    pub fn new(scale: SparklineScale) -> Self {
        Self {
            current_sparkline_scale: scale,
            color_support: detect_color_support(),
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



fn generate_colored_sparkline(hop: &crate::HopStats, global_max_rtt: u64, scale: SparklineScale, color_support: ColorSupport) -> Vec<Span<'static>> {
    if hop.sent == 0 {
        return vec![];
    }

    // Use the chronological packet history from HopStats
    hop.packet_history
        .iter()
        .map(|outcome| {
            match outcome {
                crate::hop_stats::PacketOutcome::Received(rtt) => {
                    let rtt_ms = (rtt.as_secs_f64() * 1000.0) as u64;
                    let ratio = match scale {
                        SparklineScale::Linear => {
                            rtt_ms as f64 / global_max_rtt as f64
                        }
                        SparklineScale::Logarithmic => {
                            if rtt_ms == 0 || global_max_rtt == 0 {
                                0.0
                            } else {
                                // Logarithmic scaling: log(rtt + 1) / log(max_rtt + 1)
                                ((rtt_ms + 1) as f64).ln() / ((global_max_rtt + 1) as f64).ln()
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
        .collect()
}

fn get_rtt_char_and_color(ratio: f64, color_support: ColorSupport) -> (char, Color) {
    let level = (ratio * 8.0) as usize;
    let char = match level {
        0 => ' ',
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
                0..=1 => Color::Green,      // Fast - green
                2..=3 => Color::Cyan,       // Good - cyan 
                4..=5 => Color::Yellow,     // Medium - yellow
                6..=7 => Color::Magenta,    // Slow - magenta
                _ => Color::Red,            // Very slow - red
            }
        }
        ColorSupport::Extended => {
            // Use 256-color palette for smoother gradation
            // Using colorblind-friendly blues to oranges/reds
            match level {
                0 => Color::Indexed(22),    // Dark green
                1 => Color::Indexed(28),    // Green
                2 => Color::Indexed(34),    // Light green  
                3 => Color::Indexed(40),    // Green-cyan
                4 => Color::Indexed(220),   // Yellow
                5 => Color::Indexed(214),   // Orange
                6 => Color::Indexed(208),   // Dark orange
                7 => Color::Indexed(196),   // Red
                _ => Color::Indexed(160),   // Dark red
            }
        }
        ColorSupport::TrueColor => {
            // Use RGB colors for finest gradation - colorblind safe palette
            match level {
                0 => Color::Rgb(0, 100, 0),      // Dark green
                1 => Color::Rgb(0, 150, 0),      // Green
                2 => Color::Rgb(100, 200, 0),    // Yellow-green
                3 => Color::Rgb(200, 200, 0),    // Yellow
                4 => Color::Rgb(255, 150, 0),    // Orange
                5 => Color::Rgb(255, 100, 0),    // Dark orange
                6 => Color::Rgb(255, 50, 0),     // Red-orange
                7 => Color::Rgb(200, 0, 0),      // Red
                _ => Color::Rgb(150, 0, 0),      // Dark red
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




pub fn render_ui(f: &mut Frame, session: &MtrSession, ui_state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(8),     // Main table
            Constraint::Length(5),  // RTT graph
            Constraint::Length(3),  // Status bar
        ])
        .split(f.size());

    // Header
    let header = Paragraph::new(format!(
        "mtr-ng: {} → {} ({})",
        "localhost", session.target, session.target_addr
    ))
    .block(Block::default().borders(Borders::ALL).title("Network Trace"));
    f.render_widget(header, chunks[0]);

    // Calculate global max RTT for sparkline scaling across all hops
    let global_max_rtt = session
        .hops
        .iter()
        .filter(|hop| hop.sent > 0)
        .flat_map(|hop| hop.rtts.iter())
        .map(|d| (d.as_secs_f64() * 1000.0) as u64)
        .max()
        .unwrap_or(1);

    // Main statistics table
    let items: Vec<ListItem> = session
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
                hop.addr.map(|a| a.to_string()).unwrap_or_else(|| "???".to_string())
            } else {
                hop.hostname.clone().unwrap_or_else(|| 
                    hop.addr.map(|a| a.to_string()).unwrap_or_else(|| "???".to_string())
                )
            };
            
            // Truncate hostname to fit in column (20 chars max)
            let hostname = if hostname.len() > 20 {
                format!("{}...", &hostname[..17])
            } else {
                hostname
            };

            let rtt_text = if let Some(_last) = hop.last_rtt {
                format!(
                    "{:6.1}ms {:6.1}ms {:6.1}ms {:6.1}ms",
                    hop.last_rtt.unwrap_or_default().as_secs_f64() * 1000.0,
                    hop.avg_rtt.unwrap_or_default().as_secs_f64() * 1000.0,
                    hop.best_rtt.unwrap_or_default().as_secs_f64() * 1000.0,
                    hop.worst_rtt.unwrap_or_default().as_secs_f64() * 1000.0,
                )
            } else {
                "   ???ms    ???ms    ???ms    ???ms".to_string()
            };

            // Generate colored sparkline for RTT history including lost packets
            let sparkline_spans = generate_colored_sparkline(hop, global_max_rtt, ui_state.current_sparkline_scale, ui_state.color_support);

            // Create the row with proper spacing
            let mut row_spans = vec![
                Span::styled(format!("{:2}.", hop.hop), Style::default().fg(Color::White)),
                Span::styled(format!("{:21}", hostname), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{:6.1}%", hop.loss_percent), Style::default().fg(loss_color)),
                Span::styled(format!("{:4}", hop.sent), Style::default().fg(Color::Gray)),
                Span::styled(rtt_text, Style::default().fg(Color::Yellow)),
                Span::styled(" ".to_string(), Style::default()), // Space before sparkline
            ];
            
            // Add the colored sparkline spans
            row_spans.extend(sparkline_spans);

            ListItem::new(Line::from(row_spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("  Host                   Loss%  Snt  Last    Avg   Best  Wrst  RTT Graph")
        );
    f.render_widget(list, chunks[1]);

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
            let min_rtt = data.iter().map(|(_, rtt)| *rtt).fold(f64::INFINITY, f64::min);

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
            f.render_widget(chart, chunks[2]);
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
        "Active Hops: {} | Total Sent: {} | Total Received: {} | Overall Loss: {:.1}% | Sparkline: {} | Colors: {} | Keys: 'q'=quit, 'r'=reset, 's'=scale, 'c'=colors",
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
    f.render_widget(status, chunks[3]);
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
    
    // Create UI state with initial sparkline scale from args
    let mut ui_state = UiState::new(session.args.sparkline_scale);
    
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

    // Create UI state with initial sparkline scale from args
    let mut ui_state = UiState::new(session.args.sparkline_scale);

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
    event_sender: mpsc::UnboundedSender<NetworkEvent>
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



 