use clap::Parser;
use mtr_ng::{report::run_report, ui::run_interactive, Args, MtrSession, Result};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Configure logging based on mode
    if args.report {
        // In report mode, we can safely log to stderr
        tracing_subscriber::fmt()
            .with_env_filter("mtr_ng=info")
            .with_writer(std::io::stderr)
            .init();
            
        info!("Starting mtr-ng v0.1.0 (Report Mode)");
        info!("Target: {}", args.target);
    } else {
        // In interactive mode, log to a file to avoid interfering with TUI
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("mtr-ng.log")
            .unwrap_or_else(|_| {
                // If we can't create log file, just use a null writer
                std::fs::File::create("/dev/null").expect("Failed to create null device")
            });
            
        tracing_subscriber::fmt()
            .with_env_filter("mtr_ng=debug")
            .with_writer(log_file)
            .init();
            
        info!("Starting mtr-ng v0.1.0 (Interactive Mode)");
        info!("Target: {}", args.target);
    }

    let session = MtrSession::new(args).await?;

    if session.args.report {
        run_report(session).await
    } else {
        run_interactive(session).await
    }
} 