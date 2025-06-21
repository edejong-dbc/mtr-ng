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

## Code Cleanup Guidelines

### Mandatory Cleanup Checks

Before any commit, ensure the codebase passes these checks:

```bash
# 1. Zero compilation warnings
cargo build --release

# 2. Zero clippy warnings (strict mode)
cargo clippy --release -- -D warnings

# 3. All tests pass
cargo test

# 4. Functional verification
./target/release/mtr-ng google.com --simulate -c 1 -r
```

### Systematic Cleanup Process

#### 1. **Fix All Linter Warnings**
- **Unused mut**: Add `#[allow(unused_mut)]` only when variable IS used mutably in async closures
- **Clippy warnings**: Fix immediately, never ignore
- **Dead code**: Remove completely, don't just silence warnings
- **Magic numbers**: Extract to named constants

#### 2. **Remove Unused Code**
Search and eliminate:
```bash
# Find unused imports
grep -r "^use.*;" src/ | # Manual review needed

# Find unused functions (not called anywhere)
grep -r "fn.*never_called" src/

# Find unused structs/enums
grep -r "struct.*Unused\|enum.*Unused" src/

# Find TODO/FIXME comments
grep -r "TODO\|FIXME\|XXX\|HACK" src/
```

**Removal Priority:**
1. **Backup files** (*.backup, *.bak, *.tmp)
2. **Unused types** (structs, enums never instantiated)
3. **Unused functions** (never called)
4. **Unused modules** (not imported)
5. **Dead code paths** (unreachable code)

#### 3. **Constants and Magic Numbers**
Replace hardcoded values with named constants:
```rust
// Before
let buffer = [0u8; 1500];
self.next_seq = 32768;

// After  
const MAX_MTU: usize = 1500;
const INITIAL_SEQUENCE: u16 = 32768;
let buffer = [0u8; MAX_MTU];
self.next_seq = INITIAL_SEQUENCE;
```

#### 4. **Import Cleanup**
- **No wildcard imports**: Use explicit imports instead of `use module::*`
- **Group imports**: std, external crates, local modules
- **Remove unused imports**: Let compiler/clippy guide you

```rust
// Before
use hickory_resolver::{config::*, TokioAsyncResolver};

// After
use hickory_resolver::{config::{ResolverConfig, ResolverOpts}, TokioAsyncResolver};
```

#### 5. **Code Quality Improvements**
- **Replace `.map()` with `if let`** for unit functions
- **Use `.to_string()`** instead of `format!("{}", x)`
- **Prefer explicit error handling** over `.unwrap()`
- **Add documentation** for public APIs

### Cleanup Verification

After cleanup, verify:
```bash
# 1. No warnings
cargo build --release 2>&1 | grep -E "warning:|error:" | wc -l  # Should be 0

# 2. No clippy issues  
cargo clippy --release -- -D warnings  # Should pass

# 3. Tests still pass
cargo test  # All tests green

# 4. Functionality preserved
./target/release/mtr-ng google.com --simulate -c 1 -r  # Should work

# 5. Real network mode works (if you have sudo)
sudo ./target/release/mtr-ng 8.8.8.8 -c 1 -r  # Should work
```

### File-Specific Cleanup Rules

#### **src/session.rs** (CRITICAL)
- Never remove sequence management logic
- Keep MTR algorithm fidelity intact
- Only remove clearly unused helper functions
- Test real network mode after changes

#### **src/probe.rs**
- Remove unused packet construction functions
- Keep core send/receive logic intact
- Verify ProbeEngine functionality

#### **src/ui.rs**
- Remove unused visualization modes
- Keep core rendering functions
- Test interactive mode after changes

#### **src/hop_stats.rs**
- Remove unused statistics methods
- Keep core RTT calculation logic
- Verify statistics accuracy

### Anti-Patterns to Avoid

❌ **Never do this:**
- Remove code just because it "looks unused" without verification
- Ignore clippy warnings with blanket `#[allow]` attributes
- Leave TODO comments without tickets/issues
- Keep backup files in the repository
- Use magic numbers without constants

✅ **Always do this:**
- Verify unused code is truly unused across all modules
- Fix clippy warnings properly, don't just silence them
- Replace magic numbers with named constants
- Remove backup/temporary files
- Test functionality after cleanup

### Cleanup Commit Guidelines

Structure cleanup commits like this:
```
Code cleanup: fix warnings, remove unused code, add constants

- Fix all clippy warnings (specific issues fixed)
- Remove unused types: ListSpecificTypes
- Remove unused functions: ListSpecificFunctions  
- Remove unused files: ListSpecificFiles (X lines)
- Add constants: ListNewConstants to replace magic numbers
- Fix imports: specific import improvements
- Zero warnings, all tests pass, functionality preserved
- Removed ~X lines of dead code
```

### Maintenance Schedule

**Before every commit:**
- Run `cargo clippy --release -- -D warnings`
- Check for new unused code

**Weekly cleanup:**
- Full unused code analysis
- Magic number extraction
- Import optimization

**Monthly deep cleanup:**
- Dependency audit (`cargo audit`)
- Performance profiling
- Documentation updates

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