# MTR-NG - Modern My Traceroute in Rust

A modern, feature-rich implementation of My Traceroute (MTR) built in Rust, offering real-time network path visualization with enhanced sparklines, jitter analysis, and advanced column customization.

## Features

### Core Network Analysis
- **Real-time Path Tracing**: Live network path discovery and monitoring
- **RTT Statistics**: Min/Max/Average/EMA (Exponential Moving Average) calculations  
- **Packet Loss Detection**: Accurate loss percentage tracking per hop
- **Jitter Analysis**: Last jitter and average jitter measurements for network stability

### Advanced Visualization
- **Unicode Sparklines**: Beautiful real-time RTT history visualization (`▁▂▂▆█▄▂▃▂▁`)
- **Color-coded Metrics**: Green (good) → Yellow (warning) → Red (problematic)
- **Customizable Columns**: Select exactly which metrics to display
- **Scalable Display**: Auto-scaling sparklines with manual override options

### Column Selection System
- **Flexible Fields**: Choose from 12 available metrics
  - `hop`, `host`, `loss`, `sent`, `last`, `avg`, `ema`
  - `jitter`, `jitter-avg`, `best`, `worst`, `graph`
- **Quick Presets**: `--show-all` for complete metrics, custom combinations via `--fields`
- **Interactive Toggle**: Press `f` key in interactive mode to cycle columns
- **Report Mode**: Same column selection works for both interactive and report output

### Interface Modes
- **Interactive UI**: Real-time terminal interface with keyboard controls
- **Report Mode**: Clean output for automation and scripting
- **Colorblind Friendly**: Accessible color schemes for all users

### Performance & Compatibility
- **Efficient Async**: Built with Tokio for high-performance networking
- **Cross-platform**: Works on Linux, macOS, and Windows
- **IPv4 Support**: Robust IP address handling and DNS resolution
- **Terminal Detection**: Automatic capability detection and graceful fallbacks

## Installation

### Quick Install (Recommended)
```bash
# Unix/Linux/macOS one-liner
curl -sSL https://raw.githubusercontent.com/edejong-dbc/mtr-ng/main/install/install.sh | bash
```

### Package Managers

#### Homebrew (macOS)
```bash
# Option 1: Using the tap (recommended)
brew tap edejong-dbc/tap
brew install mtr-ng

# Option 2: Direct install without adding tap
brew install edejong-dbc/tap/mtr-ng
```

#### Cargo (Rust)
```bash
cargo install mtr-ng
```

#### Arch Linux (AUR)
```bash
yay -S mtr-ng
```

#### Debian/Ubuntu
Download the `.deb` package from [releases](https://github.com/edejong-dbc/mtr-ng/releases):
```bash
sudo dpkg -i mtr-ng_*.deb
```

### From Source
```bash
# Clone and build
git clone https://github.com/edejong-dbc/mtr-ng
cd mtr-ng
cargo build --release

# Install
sudo cp target/release/mtr-ng /usr/local/bin/
sudo cp install/mtr-ng.1 /usr/local/share/man/man1/
```

## Usage Examples

### Basic Usage
```bash
# Standard trace with default columns
mtr-ng google.com

# Show all available metrics
mtr-ng google.com --show-all

# Custom column selection
mtr-ng google.com --fields hop,host,loss,last,avg,graph

# Focus on jitter analysis  
mtr-ng google.com --fields hop,host,jitter,jitter-avg,graph
```

### Interactive Mode
```bash
# Real-time monitoring with keyboard controls
mtr-ng google.com

# Controls:
# q/Esc - Quit
# r     - Reset statistics  
# s     - Toggle sparkline scale
# c     - Cycle color modes
# f     - Toggle column visibility
```

### Report Mode
```bash
# Generate report output
mtr-ng google.com --report

# Automation-friendly format
mtr-ng google.com --report --fields hop,host,loss,avg > network_report.txt
```

## Advanced Features

### Sparkline Visualization
MTR-NG provides beautiful Unicode sparklines showing RTT history:
- `▁▂▃▄▅▆▇█` - Visual representation of network performance
- Color coding: Green (fast) → Yellow (moderate) → Red (slow)
- Real-time updates as packets are sent/received

### Jitter Analysis  
Monitor network stability with comprehensive jitter metrics:
- **Last Jitter**: Most recent RTT variation
- **Average Jitter**: Long-term jitter trends
- Helps identify unstable network segments



## Column Reference

| Column      | Description                    | Example  |
|-------------|--------------------------------|----------|
| `hop`       | Hop number                     | `1`      |
| `host`      | Hostname/IP address            | `gateway.local` |
| `loss`      | Packet loss percentage         | `2.0%`   |
| `sent`      | Packets sent                   | `10`     |
| `last`      | Most recent RTT               | `15.2ms` |
| `avg`       | Average RTT                   | `18.4ms` |
| `ema`       | Exponential moving average    | `17.8ms` |
| `jitter`    | Last jitter value             | `2.1ms`  |
| `jitter-avg`| Average jitter                | `1.8ms`  |
| `best`      | Minimum RTT observed          | `12.1ms` |
| `worst`     | Maximum RTT observed          | `45.2ms` |
| `graph`     | RTT sparkline visualization   | `▁▂▄▇▆▃▁` |

## Development Status

MTR-NG is actively developed with focus on:
- **Stable Core**: Reliable network tracing and statistics
- **Rich Visualization**: Unicode sparklines and color coding  
- **Column Flexibility**: Complete customization system
- **Interactive UI**: Real-time monitoring with controls
- **IPv6 Support**: Next-generation protocol support (planned)

## Contributing

Contributions welcome! Areas of interest:
- Terminal graphics and visualization improvements
- IPv6 protocol support
- Additional statistical metrics
- Platform-specific optimizations

## License

MIT License - see LICENSE file for details. 

# edejong-dbc/homebrew-tap

This is a Homebrew tap for [mtr-ng](https://github.com/edejong-dbc/mtr-ng), a modern implementation of My Traceroute with real-time network path visualization.

## Installation

```bash
# Add the tap
brew tap edejong-dbc/tap

# Install mtr-ng
brew install mtr-ng
```

## Usage

```bash
# Basic usage (requires sudo for raw socket access)
sudo mtr-ng google.com

# Show all metrics with sparklines
sudo mtr-ng google.com --show-all

# Generate a report
mtr-ng google.com --report --count 10
```

## About mtr-ng

MTR-NG is a modern, feature-rich implementation of My Traceroute (MTR) that provides:

- Real-time network path visualization with Unicode sparklines
- Comprehensive RTT statistics and jitter analysis
- Customizable column display and interactive controls
- Cross-platform support with colorblind-friendly design

For more information, visit the [main repository](https://github.com/edejong-dbc/mtr-ng). 