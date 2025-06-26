use crate::args::Column;
use crate::utils;
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
            Column::Last => header.push_str("   Last"),
            Column::Avg => header.push_str("    Avg"),
            Column::Ema => header.push_str("   EMA"),
            Column::Jitter => header.push_str("  Jttr"),
            Column::JitterAvg => header.push_str("  JAvg"),
            Column::Best => header.push_str("  Best"),
            Column::Worst => header.push_str("  Wrst"),
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
                    row.push_str(&format!(" {:6.1}", utils::time::duration_to_ms_f64(rtt)));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Avg => {
                if let Some(rtt) = hop.avg_rtt {
                    row.push_str(&format!(" {:6.1}", utils::time::duration_to_ms_f64(rtt)));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Ema => {
                if let Some(rtt) = hop.ema_rtt {
                    row.push_str(&format!(" {:5.1}", utils::time::duration_to_ms_f64(rtt)));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Jitter => {
                if let Some(jitter) = hop.last_jitter {
                    row.push_str(&format!(" {:5.1}", utils::time::duration_to_ms_f64(jitter)));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::JitterAvg => {
                if let Some(jitter) = hop.jitter_avg {
                    row.push_str(&format!(" {:5.1}", utils::time::duration_to_ms_f64(jitter)));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Best => {
                if let Some(rtt) = hop.best_rtt {
                    row.push_str(&format!(" {:5.1}", utils::time::duration_to_ms_f64(rtt)));
                } else {
                    row.push_str("   ???");
                }
            }
            Column::Worst => {
                if let Some(rtt) = hop.worst_rtt {
                    row.push_str(&format!(" {:5.1}", utils::time::duration_to_ms_f64(rtt)));
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

    // Determine how many hops to display based on discovery or organic growth  
    let max_hops_to_display = if session.num_hosts > 0 {
        session.num_hosts
    } else {
        // Show all hops that have been probed
        session.hops.iter()
            .enumerate()
            .rev()
            .find(|(_, hop)| hop.sent > 0)
            .map(|(i, _)| i + 1)
            .unwrap_or(0)
    };
    
    for hop in session.hops.iter().take(max_hops_to_display) {
        if hop.sent == 0 {
            continue;
        }

        let hostname = if session.args.numeric {
            utils::network::format_optional_ip(hop.addr)
        } else {
            utils::network::format_hostname_with_fallback(hop.hostname.clone(), hop.addr)
        };

        let stddev = if hop.received > 1 && hop.rtts.len() > 1 {
            let mean = utils::time::duration_to_ms_f64(hop.avg_rtt.unwrap());
            let rtt_values_ms: Vec<f64> = hop
                .rtts
                .iter()
                .map(|rtt| utils::time::duration_to_ms_f64(*rtt))
                .collect();
            utils::math::calculate_stddev(&rtt_values_ms, mean)
        } else {
            0.0
        };

        println!("{}", format_row_data(hop, &hostname, &columns, stddev));
    }

    Ok(())
}
