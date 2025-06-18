# MTR-NG Development Instructions

## Project Overview

MTR-NG is a high-performance network traceroute tool written in Rust, designed to be a modern replacement for the classic MTR tool. It implements the exact same algorithm as the original MTR but with improved performance and accuracy.

## Architecture

### Core Components

1. **`src/main.rs`** - CLI entry point and argument parsing
2. **`src/session.rs`** - Core MTR algorithm implementation (MOST CRITICAL)
3. **`src/ui.rs`** - TUI interface using crossterm
4. **`src/report.rs`** - Report mode output formatting
5. **`src/hop_stats.rs`** - Statistics tracking per hop
6. **`src/args.rs`** - Command line argument definitions

### Key Algorithm Implementation (session.rs)

The `session.rs` file implements the exact MTR algorithm from the original C code:

#### Critical Data Structures:
- **`SequenceEntry`** - Tracks individual packets in transit (like C `struct sequence`)
- **`sequence_table`** - HashMap tracking active packets by sequence number
- **`batch_at`** - Current hop index being probed
- **`num_hosts`** - Number of discovered hops

#### Core Algorithm Flow:
1. **`run_mtr_algorithm()`** - Main loop
2. **`net_send_batch()`** - Send one packet per round (like C `net_send_batch`)
3. **`new_sequence()`/`save_sequence()`** - Packet tracking
4. **`net_process_return()`** - Response collection
5. **`mark_sequence_complete()`** - Match responses to sent packets
6. **`net_process_ping()`** - RTT calculation and statistics

## Development Workflow

### Building and Testing

```bash
# Build the project
cargo build

# Run with privileges (required for raw sockets)
sudo cargo run -- google.com

# Run in report mode
sudo cargo run -- -c 10 -r google.com

# Run with debug logging
RUST_LOG=debug sudo cargo run -- google.com
```

### Performance Testing

Always compare against original mtr:
```bash
# Test our implementation
sudo cargo run -- -c 3 -r google.com

# Compare with original
sudo mtr -c 3 -r google.com
```

**Target Performance**: RTT measurements should be within 1.5-2x of original mtr.

## Critical Implementation Details

### RTT Timing Bug (FIXED)
- **Problem**: Previous implementation had 8x timing error
- **Root Cause**: Measuring wrong time intervals in batch processing
- **Solution**: Store exact send time in `SequenceEntry`, calculate RTT on response
- **Result**: Now within 1.4-1.6x of original mtr accuracy

### Sequence Management
- Use sequence numbers 33000-65535 (like original)
- Store send time when packet is sent
- Remove from table when response received
- Calculate RTT = receive_time - send_time

### Socket Handling
- Raw ICMP sockets require root privileges
- Non-blocking sockets with 1ms timeout
- TTL set per packet for hop limiting
- Proper ICMP packet parsing and validation

## Known Issues and TODOs

### Current Limitations:
1. **IPv6 Support** - Not yet implemented (falls back to simulation)
2. **Hostname Resolution** - Currently shows IP addresses only
3. **MPLS Support** - Not implemented
4. **Advanced Features** - Missing some original mtr features

### Performance TODOs:
1. **Multi-threading** - Could parallelize response collection
2. **Memory Pool** - Reuse packet buffers
3. **Zero-copy Parsing** - Avoid buffer copies where possible

### Future Enhancements:
1. **JSON Output** - Add structured output format
2. **Prometheus Metrics** - Export metrics for monitoring
3. **HTTP API** - Web interface for remote monitoring
4. **Custom Packet Sizes** - Support variable packet sizes

## Testing Strategy

### Unit Tests
- Test sequence management logic
- Test RTT calculation accuracy
- Test packet parsing functions

### Integration Tests
- Compare output with original mtr
- Test various network conditions
- Test error handling (permissions, network unreachable)

### Performance Tests
- Measure RTT accuracy vs original mtr
- Test packet loss handling
- Measure memory usage and CPU performance

## Debugging Tips

### Common Issues:
1. **Permission Denied** - Run with `sudo` for raw sockets
2. **High RTT Values** - Check sequence tracking logic
3. **Missing Responses** - Verify ICMP parsing
4. **Timeouts** - Adjust response collection window

### Debug Logging:
```bash
RUST_LOG=debug sudo cargo run -- target
```

Key debug messages to watch:
- "Sending probe: hop=X, TTL=Y, seq=Z"
- "Hop X: Got TimeExceeded from Y in Z.Zms"
- "Saved sequence: seq=X, hop=Y"

### Performance Profiling:
```bash
# Use perf for detailed profiling
sudo perf record --call-graph dwarf cargo run -- -c 100 -r google.com
perf report
```

## Code Style and Best Practices

### Rust Guidelines:
- Use `cargo clippy` for linting
- Format with `cargo fmt`
- Add comprehensive error handling
- Use proper lifetime management for network buffers

### Network Code Best Practices:
- Always validate packet contents
- Handle partial reads/writes
- Use non-blocking I/O with proper timeouts
- Clean up resources (sockets, buffers)

### MTR Algorithm Fidelity:
- Keep algorithm as close to original C code as possible
- Use same constants and timing intervals
- Maintain same statistics calculation methods
- Preserve CLI compatibility

## Deployment Considerations

### System Requirements:
- Linux/macOS with raw socket support
- Root privileges or CAP_NET_RAW capability
- Rust 1.70+ for compilation

### Distribution:
- Static binary compilation for portability
- Docker container for isolated deployment
- Package managers (apt, brew, cargo install)

## Maintenance Notes

### Critical Files to Monitor:
1. **`src/session.rs`** - Core algorithm, handle with extreme care
2. **`src/hop_stats.rs`** - Statistics accuracy
3. **`Cargo.toml`** - Dependency management

### Version Compatibility:
- Maintain CLI compatibility with original mtr
- Output format should match for script compatibility
- Performance should improve or stay within 2x of original

### Security Considerations:
- Raw socket usage requires privileges
- Validate all network input data
- Prevent buffer overflows in packet parsing
- Rate limit outgoing packets to prevent network abuse

---

## Quick Reference

### Build Commands:
```bash
cargo build                    # Debug build
cargo build --release          # Optimized build
cargo test                     # Run tests
cargo clippy                   # Lint check
```

### Run Commands:
```bash
sudo ./target/debug/mtr-ng google.com           # Interactive mode
sudo ./target/debug/mtr-ng -c 10 -r google.com  # Report mode
sudo ./target/debug/mtr-ng --help               # Show help
```

### Performance Targets:
- RTT accuracy: Within 2x of original mtr
- Packet rate: Match or exceed original mtr
- Memory usage: < 10MB for typical usage
- CPU usage: < 5% for continuous monitoring 