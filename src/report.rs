use crate::{MtrSession, Result};

pub async fn run_report(mut session: MtrSession) -> Result<()> {
    session.run_trace().await?;

    println!("Start: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    println!("HOST: {} → {} ({})", "localhost", session.target, session.target_addr);
    println!();
    println!("                             Loss%   Snt   Last   Avg  Best  Wrst StDev");

    for hop in &session.hops {
        if hop.sent == 0 {
            continue;
        }

        let hostname = if session.args.numeric {
            hop.addr.map(|a| a.to_string()).unwrap_or_else(|| "???".to_string())
        } else {
            hop.hostname.clone().unwrap_or_else(|| 
                hop.addr.map(|a| a.to_string()).unwrap_or_else(|| "???".to_string())
            )
        };

        if hop.received > 0 {
            let stddev = if hop.rtts.len() > 1 {
                let mean = hop.avg_rtt.unwrap().as_secs_f64() * 1000.0;
                let variance = hop.rtts.iter()
                    .map(|rtt| {
                        let diff = rtt.as_secs_f64() * 1000.0 - mean;
                        diff * diff
                    })
                    .sum::<f64>() / (hop.rtts.len() - 1) as f64;
                variance.sqrt()
            } else {
                0.0
            };

            println!(
                "{:2}.|-- {:20} {:5.1}% {:4} {:6.1} {:6.1} {:5.1} {:5.1} {:5.1}",
                hop.hop,
                hostname,
                hop.loss_percent,
                hop.sent,
                hop.last_rtt.unwrap().as_secs_f64() * 1000.0,
                hop.avg_rtt.unwrap().as_secs_f64() * 1000.0,
                hop.best_rtt.unwrap().as_secs_f64() * 1000.0,
                hop.worst_rtt.unwrap().as_secs_f64() * 1000.0,
                stddev,
            );
        } else {
            println!(
                "{:2}.|-- {:20} {:5.1}% {:4}   ???    ???   ???   ???   ???",
                hop.hop, hostname, hop.loss_percent, hop.sent
            );
        }
    }

    Ok(())
} 