use crate::MtrSession;
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
use crate::{HopStats, Result};


pub fn render_ui(f: &mut Frame, session: &MtrSession) {
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

            // Unicode sparkline for RTT history
            let sparkline_data: Vec<u64> = hop.rtts
                .iter()
                .map(|d| (d.as_secs_f64() * 1000.0) as u64)
                .collect();
            
            let sparkline = if !sparkline_data.is_empty() {
                sparkline_data
                    .iter()
                    .map(|&rtt| {
                        let ratio = rtt as f64 / global_max_rtt as f64;
                        match (ratio * 8.0) as usize {
                            0 => ' ',
                            1 => '▁',
                            2 => '▂',
                            3 => '▃',
                            4 => '▄',
                            5 => '▅',
                            6 => '▆',
                            7 => '▇',
                            _ => '█',
                        }
                    })
                    .collect::<String>()
            } else {
                "".to_string()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{:2}.", hop.hop), Style::default().fg(Color::White)),
                Span::styled(format!("{:21}", hostname), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{:6.1}%", hop.loss_percent), Style::default().fg(loss_color)),
                Span::styled(format!("{:4}", hop.sent), Style::default().fg(Color::Gray)),
                Span::styled(rtt_text, Style::default().fg(Color::Yellow)),
                Span::styled(format!(" {}", sparkline), Style::default().fg(Color::Magenta)),
            ]))
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
    
    let status_text = format!(
        "Active Hops: {} | Total Sent: {} | Total Received: {} | Overall Loss: {:.1}% | Keys: 'q'=quit, 'r'=reset",
        active_hops, total_sent, total_received, overall_loss
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

async fn run_single_trace_round(session_arc: &Arc<Mutex<MtrSession>>, round: usize) {
    debug!("Round {}", round + 1);
    
    // Extract necessary values to avoid holding mutex across await - currently not used but kept for future enhancements
    
    // Create a temporary session for this trace round
    let temp_session = {
        let session = session_arc.lock().unwrap();
        session.clone()
    };
    
    // Run one round of the trace
    let mut temp_session = temp_session;
    // Modify count to just run one round
    temp_session.args.count = 1;
    
    if let Ok(()) = temp_session.run_trace().await {
        // Update the shared session with results
        let mut session = session_arc.lock().unwrap();
        for (i, hop) in temp_session.hops.into_iter().enumerate() {
            if i < session.hops.len() {
                // Update all hop data (don't condition on sent count since simulation works differently)
                session.hops[i].sent = hop.sent;
                session.hops[i].received = hop.received;
                session.hops[i].rtts = hop.rtts;
                session.hops[i].last_rtt = hop.last_rtt;
                session.hops[i].avg_rtt = hop.avg_rtt;
                session.hops[i].best_rtt = hop.best_rtt;
                session.hops[i].worst_rtt = hop.worst_rtt;
                session.hops[i].loss_percent = hop.loss_percent;
                if hop.addr.is_some() {
                    session.hops[i].addr = hop.addr;
                }
                if hop.hostname.is_some() {
                    session.hops[i].hostname = hop.hostname;
                }
            }
        }
    }
}

pub async fn run_interactive(session: MtrSession) -> Result<()> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Shared state for the UI and trace runner
    let session = Arc::new(Mutex::new(session));
    let session_clone = Arc::clone(&session);

    // Start the trace in a background task
    let trace_handle = tokio::spawn(async move {
        let count = session_clone.lock().unwrap().args.count;
        let interval = session_clone.lock().unwrap().args.interval;
        
        // We need to run the trace in chunks to periodically update the shared state
        for round in 0..count {
            // Run trace round asynchronously
            run_single_trace_round(&session_clone, round).await;
            
            // Sleep between rounds (cap at 500ms for better demo experience)
            let sleep_time = std::cmp::min(interval, 500);
            tokio::time::sleep(Duration::from_millis(sleep_time)).await;
        }
    });

    // Main UI loop
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100); // More responsive updates

    loop {
        let session_guard = session.lock().unwrap();
        terminal.draw(|f| render_ui(f, &session_guard))?;
        drop(session_guard);

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => {
                        // Reset statistics
                        let mut session_guard = session.lock().unwrap();
                        for hop in &mut session_guard.hops {
                            *hop = HopStats::new(hop.hop);
                        }
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
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