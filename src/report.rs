use crate::args::Column;
use crate::{MtrSession, Result};

fn format_column_headers(columns: &[Column]) -> String {
    let mut header = String::new();
    for (i, column) in columns.iter().enumerate() {
        if i > 0 {
            header.push(' ');
        }
        match column {
            Column::Hop => {}  // No header padding needed
            Column::Host => {} // No header padding needed
            Column::Loss => header.push_str("Loss%"),
            Column::Sent => header.push_str(" Snt"),
            Column::Last => header.push_str("    Last"),
            Column::Avg => header.push_str("     Avg"),
            Column::Ema => header.push_str("    EMA"),
            Column::Jitter => header.push_str("   Jttr"),
            Column::JitterAvg => header.push_str("   JAvg"),
            Column::Best => header.push_str("   Best"),
            Column::Worst => header.push_str("   Wrst"),
            Column::Graph => header.push_str("StDev"), // Use StDev for report mode instead of graph
        }
    }
    header
}

fn format_row_data(
    hop: &crate::HopStats,
    hostname: &str,
    columns: &[Column],
    stddev: f64,
) -> String {
    let mut row = String::new();
    for (i, column) in columns.iter().enumerate() {
        if i > 0 {
            row.push(' ');
        }
        match column {
            Column::Hop => row.push_str(&format!("{:2}.|--", hop.hop)),
            Column::Host => row.push_str(&format!(" {:20}", hostname)),
            Column::Loss => row.push_str(&format!(" {:5.1}%", hop.loss_percent)),
            Column::Sent => row.push_str(&format!(" {:4}", hop.sent)),
            Column::Last => {
                if let Some(rtt) = hop.last_rtt {
                    row.push_str(&format!(" {:6.1}", rtt.as_secs_f64() * 1000.0));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Avg => {
                if let Some(rtt) = hop.avg_rtt {
                    row.push_str(&format!(" {:6.1}", rtt.as_secs_f64() * 1000.0));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Ema => {
                if let Some(rtt) = hop.ema_rtt {
                    row.push_str(&format!(" {:5.1}", rtt.as_secs_f64() * 1000.0));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Jitter => {
                if let Some(jitter) = hop.last_jitter {
                    row.push_str(&format!(" {:5.1}", jitter.as_secs_f64() * 1000.0));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::JitterAvg => {
                if let Some(jitter) = hop.jitter_avg {
                    row.push_str(&format!(" {:5.1}", jitter.as_secs_f64() * 1000.0));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Best => {
                if let Some(rtt) = hop.best_rtt {
                    row.push_str(&format!(" {:5.1}", rtt.as_secs_f64() * 1000.0));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Worst => {
                if let Some(rtt) = hop.worst_rtt {
                    row.push_str(&format!(" {:5.1}", rtt.as_secs_f64() * 1000.0));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Graph => {
                row.push_str(&format!(" {:5.1}", stddev));
            }
        }
    }
    row
}

pub async fn run_report(mut session: MtrSession) -> Result<()> {
    session.run_trace().await?;

    let columns = session.args.get_columns();

    println!(
        "Start: {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!(
        "HOST: localhost → {} ({})",
        session.target, session.target_addr
    );
    println!();
    println!(
        "                             {}",
        format_column_headers(&columns)
    );

    for hop in &session.hops {
        if hop.sent == 0 {
            continue;
        }

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

        let stddev = if hop.received > 1 && hop.rtts.len() > 1 {
            let mean = hop.avg_rtt.unwrap().as_secs_f64() * 1000.0;
            let variance = hop
                .rtts
                .iter()
                .map(|rtt| {
                    let diff = rtt.as_secs_f64() * 1000.0 - mean;
                    diff * diff
                })
                .sum::<f64>()
                / (hop.rtts.len() - 1) as f64;
            variance.sqrt()
        } else {
            0.0
        };

        println!("{}", format_row_data(hop, &hostname, &columns, stddev));
    }

    Ok(())
}
