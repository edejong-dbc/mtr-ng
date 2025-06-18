use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "mtr-ng")]
#[command(about = "A modern implementation of mtr (My Traceroute) with unicode and terminal graphics")]
#[command(version = "0.1.0")]
pub struct Args {
    /// Target hostname or IP address
    pub target: String,

    /// Number of pings per round (default: infinite)
    #[arg(short, long)]
    pub count: Option<usize>,

    /// Wait time between pings in milliseconds
    #[arg(short, long, default_value = "1000")]
    pub interval: u64,

    /// Maximum number of hops
    #[arg(short = 'M', long, default_value = "30")]
    pub max_hops: u8,

    /// Enable report mode (non-interactive)
    #[arg(short, long)]
    pub report: bool,

    /// Show IP addresses instead of hostnames
    #[arg(short, long)]
    pub numeric: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_default_values() {
        let args = Args::try_parse_from(&["mtr-ng", "example.com"]).unwrap();
        assert_eq!(args.target, "example.com");
        assert_eq!(args.count, None); // Default is now infinite (None)
        assert_eq!(args.interval, 1000);
        assert_eq!(args.max_hops, 30);
        assert!(!args.report);
        assert!(!args.numeric);
    }

    #[test]
    fn test_args_custom_values() {
        let args = Args::try_parse_from(&[
            "mtr-ng",
            "--count", "20",
            "--interval", "500", 
            "--max-hops", "50",
            "--report",
            "--numeric",
            "google.com"
        ]).unwrap();
        
        assert_eq!(args.target, "google.com");
        assert_eq!(args.count, Some(20));
        assert_eq!(args.interval, 500);
        assert_eq!(args.max_hops, 50);
        assert!(args.report);
        assert!(args.numeric);
    }

    #[test]
    fn test_args_short_flags() {
        let args = Args::try_parse_from(&[
            "mtr-ng",
            "-c", "15",
            "-i", "2000",
            "-M", "25",
            "-r",
            "-n",
            "test.example.com"
        ]).unwrap();
        
        assert_eq!(args.target, "test.example.com");
        assert_eq!(args.count, Some(15));
        assert_eq!(args.interval, 2000);
        assert_eq!(args.max_hops, 25);
        assert!(args.report);
        assert!(args.numeric);
    }
} 