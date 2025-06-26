# Justfile for mtr-ng development and testing
# Install with: cargo install just
# Run with: just <command>

# Default recipe lists all available commands
default:
    @just --list

# Run all tests without sudo
test:
    @echo "🧪 Running all tests (no sudo required)"
    cargo test
    @echo "✅ All unit tests passed!"

# Quick simulation test
sim target="google.com":
    @echo "🔬 Running simulation for {{target}}"
    cargo run -- --simulate --count 3 --report {{target}}

# Interactive simulation (press 'q' to quit)
demo target="google.com":
    @echo "🎮 Starting interactive simulation (press 'q' to quit)"
    cargo run -- --simulate {{target}}

# Fast simulation test for CI/development
quick target="8.8.8.8":
    cargo run -- --simulate --count 1 --interval 100 --report {{target}}

# Test all protocols in simulation mode
protocols:
    @echo "🌐 Testing all protocols in simulation mode"
    cargo run -- --simulate --protocol icmp --count 1 --report google.com
    cargo run -- --simulate --protocol udp --count 1 --report google.com  
    cargo run -- --simulate --protocol tcp --count 1 --report google.com

# Test different output formats
formats:
    @echo "📊 Testing different output formats"
    cargo run -- --simulate --count 2 --report --fields hop,host,loss,avg google.com
    cargo run -- --simulate --count 2 --report --show-all google.com
    cargo run -- --simulate --count 2 --report --numeric google.com

# Run the comprehensive test suite
test-all:
    @echo "🚀 Running comprehensive test suite"
    ./scripts/test.sh

# Development build and test cycle
dev:
    cargo build
    cargo test
    just sim

# Release build and test
release:
    cargo build --release
    cargo test
    @echo "🎯 Testing release build performance"
    time ./target/release/mtr-ng --simulate --count 10 --interval 50 --report google.com

# Check code quality
lint:
    cargo clippy -- -D warnings
    cargo fmt --check

# Fix code formatting
fmt:
    cargo fmt

# Build documentation
docs:
    cargo doc --open

# Clean and rebuild everything
clean:
    cargo clean
    cargo build
    cargo test

# Compare with original mtr (requires sudo and mtr to be installed)
compare target="google.com":
    @echo "📈 Comparing with original mtr (requires sudo)"
    @echo "Our implementation:"
    sudo cargo run -- --count 3 --report {{target}} || cargo run -- --simulate --count 3 --report {{target}}
    @echo "\nOriginal mtr:"
    sudo mtr -c 3 -r {{target}} || echo "❌ Original mtr not installed"

# Show help for all CLI options
help:
    cargo run -- --help

# Run a specific test pattern
test-pattern pattern:
    cargo test {{pattern}}

# Performance benchmark in simulation mode
bench:
    @echo "⚡ Performance benchmark (simulation mode)"
    time cargo run --release -- --simulate --count 100 --interval 10 --report google.com > /dev/null
    @echo "✅ Benchmark completed"

# Check for security vulnerabilities
audit:
    cargo audit

# Full CI pipeline (what runs in GitHub Actions)
ci: test lint audit
    @echo "✅ CI pipeline completed successfully"

# Create a test report
report:
    @echo "📋 Generating test report"
    cargo test 2>&1 | tee test-output.txt
    just quick 2>&1 | tee -a test-output.txt
    @echo "📄 Test report saved to test-output.txt" 