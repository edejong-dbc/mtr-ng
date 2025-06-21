use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum SparklineScale {
    Linear,
    Logarithmic,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum ProbeProtocol {
    /// ICMP Echo (ping) - default and most common
    Icmp,
    /// UDP probes (useful for firewalls that block ICMP)
    Udp,
    /// TCP SYN probes (useful for strict firewalls)
    Tcp,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum Column {
    /// Hop number
    Hop,
    /// Hostname/IP address
    Host,
    /// Packet loss percentage
    Loss,
    /// Number of packets sent
    Sent,
    /// Last RTT measurement
    Last,
    /// Average RTT
    Avg,
    /// Exponential moving average RTT
    Ema,
    /// Last jitter value
    Jitter,
    /// Average jitter
    JitterAvg,
    /// Best (minimum) RTT
    Best,
    /// Worst (maximum) RTT
    Worst,
    /// RTT sparkline graph
    Graph,
}

impl Column {
    /// Get all available columns in default order
    pub fn all() -> Vec<Column> {
        vec![
            Column::Hop,
            Column::Host,
            Column::Loss,
            Column::Sent,
            Column::Last,
            Column::Avg,
            Column::Ema,
            Column::Jitter,
            Column::JitterAvg,
            Column::Best,
            Column::Worst,
            Column::Graph,
        ]
    }

    /// Get default columns (excludes jitter by default for backwards compatibility)
    pub fn default_columns() -> Vec<Column> {
        vec![
            Column::Hop,
            Column::Host,
            Column::Loss,
            Column::Sent,
            Column::Last,
            Column::Avg,
            Column::Ema,
            Column::Best,
            Column::Worst,
            Column::Graph,
        ]
    }

    /// Get column header text
    pub fn header(&self) -> &'static str {
        match self {
            Column::Hop => "",
            Column::Host => "Hostname",
            Column::Loss => "Loss%",
            Column::Sent => "Pkts",
            Column::Last => "LastRTT",
            Column::Avg => "AvgRTT",
            Column::Ema => "EmaRTT",
            Column::Jitter => "Jitter",
            Column::JitterAvg => "JitAvg",
            Column::Best => "BestRTT",
            Column::Worst => "WorstRTT",
            Column::Graph => "RTT History",
        }
    }

    /// Get column width for formatting
    pub fn width(&self) -> usize {
        match self {
            Column::Hop => 3,
            Column::Host => 21,
            Column::Loss => 7,
            Column::Sent => 4,
            Column::Last => 8,
            Column::Avg => 8,
            Column::Ema => 8,
            Column::Jitter => 8,
            Column::JitterAvg => 8,
            Column::Best => 8,
            Column::Worst => 8,
            Column::Graph => 20, // Minimum width for sparkline
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "mtr-ng")]
#[command(
    about = "A modern implementation of mtr (My Traceroute) with unicode and terminal graphics"
)]
#[command(version = env!("CARGO_PKG_VERSION"))]
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

    /// Sparkline scaling mode: linear or logarithmic (default: logarithmic)
    #[arg(long, value_enum, default_value = "logarithmic")]
    pub sparkline_scale: SparklineScale,

    /// Exponential smoothing factor for EMA (0.0-1.0). Higher values = more responsive to recent changes
    #[arg(long, default_value = "0.1")]
    pub ema_alpha: f64,

    /// Select which columns to display (default: hop,host,loss,sent,last,avg,ema,best,worst,graph)
    #[arg(long, value_enum, value_delimiter = ',')]
    pub fields: Option<Vec<Column>>,

    /// Show all available columns including jitter metrics
    #[arg(long, help = "Display all available columns")]
    pub show_all: bool,

    /// Enable simulation mode (generate fake network data for testing/demo)
    #[arg(long, help = "Run in simulation mode with fake network data")]
    pub simulate: bool,

    /// Probe protocol to use for measurements  
    #[arg(short = 'P', long, value_enum, default_value = "icmp")]
    pub protocol: ProbeProtocol,
}

impl Args {
    /// Get the columns to display based on command-line arguments
    pub fn get_columns(&self) -> Vec<Column> {
        if self.show_all {
            Column::all()
        } else if let Some(ref fields) = self.fields {
            fields.clone()
        } else {
            Column::default_columns()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_default_values() {
        let args = Args::try_parse_from(["mtr-ng", "example.com"]).unwrap();
        assert_eq!(args.target, "example.com");
        assert_eq!(args.count, None); // Default is now infinite (None)
        assert_eq!(args.interval, 1000);
        assert_eq!(args.max_hops, 30);
        assert!(!args.report);
        assert!(!args.numeric);
        assert_eq!(args.sparkline_scale, SparklineScale::Logarithmic);
        assert_eq!(args.ema_alpha, 0.1);
        assert!(args.fields.is_none());
        assert!(!args.show_all);
        assert!(!args.simulate);
    }

    #[test]
    fn test_args_custom_values() {
        let args = Args::try_parse_from([
            "mtr-ng",
            "--count",
            "20",
            "--interval",
            "500",
            "--max-hops",
            "50",
            "--report",
            "--numeric",
            "google.com",
        ])
        .unwrap();

        assert_eq!(args.target, "google.com");
        assert_eq!(args.count, Some(20));
        assert_eq!(args.interval, 500);
        assert_eq!(args.max_hops, 50);
        assert!(args.report);
        assert!(args.numeric);
        assert_eq!(args.sparkline_scale, SparklineScale::Logarithmic);
        assert_eq!(args.ema_alpha, 0.1);
        assert!(args.fields.is_none());
        assert!(!args.show_all);
    }

    #[test]
    fn test_args_short_flags() {
        let args = Args::try_parse_from([
            "mtr-ng",
            "-c",
            "15",
            "-i",
            "2000",
            "-M",
            "25",
            "-r",
            "-n",
            "test.example.com",
        ])
        .unwrap();

        assert_eq!(args.target, "test.example.com");
        assert_eq!(args.count, Some(15));
        assert_eq!(args.interval, 2000);
        assert_eq!(args.max_hops, 25);
        assert!(args.report);
        assert!(args.numeric);
        assert_eq!(args.sparkline_scale, SparklineScale::Logarithmic);
        assert_eq!(args.ema_alpha, 0.1);
        assert!(args.fields.is_none());
        assert!(!args.show_all);
    }
}
