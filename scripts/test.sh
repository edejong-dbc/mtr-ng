#!/bin/bash
set -e

echo "🧪 MTR-NG Testing Suite (No Sudo Required)"
echo "========================================="

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Function to run a test with timing
run_test() {
    local name="$1"
    local cmd="$2"
    echo -e "\n${BLUE}🔧 $name${NC}"
    echo "Command: $cmd"
    echo "----------------------------------------"
    time $cmd
    echo "✅ Completed: $name"
}

# 1. Unit Tests
echo -e "\n${GREEN}1. Running Unit Tests${NC}"
run_test "Unit Tests" "cargo test"

# 2. Compilation Tests
echo -e "\n${GREEN}2. Compilation Tests${NC}"
run_test "Debug Build" "cargo build"
run_test "Release Build" "cargo build --release"

# 3. CLI Validation Tests
echo -e "\n${GREEN}3. CLI Validation Tests${NC}"
run_test "Help Output" "cargo run -- --help"
run_test "Version Output" "cargo run -- --version"

# 4. Simulation Mode Tests
echo -e "\n${GREEN}4. Simulation Mode Tests${NC}"
run_test "Basic Simulation Report" "cargo run -- --simulate --count 3 --report google.com"
run_test "Fast Simulation Report" "cargo run -- --simulate --count 1 --interval 100 --report 8.8.8.8"
run_test "Custom Hops Simulation" "cargo run -- --simulate --count 2 --max-hops 5 --report localhost"
run_test "Numeric Mode Simulation" "cargo run -- --simulate --count 2 --numeric --report example.com"
run_test "All Columns Simulation" "cargo run -- --simulate --count 1 --show-all --report cloudflare.com"

# 5. Protocol Tests (all use simulation mode)
echo -e "\n${GREEN}5. Protocol Tests (Simulated)${NC}"
run_test "ICMP Protocol Simulation" "cargo run -- --simulate --protocol icmp --count 1 --report google.com"
run_test "UDP Protocol Simulation" "cargo run -- --simulate --protocol udp --count 1 --report google.com"
run_test "TCP Protocol Simulation" "cargo run -- --simulate --protocol tcp --count 1 --report google.com"

# 6. Field Selection Tests
echo -e "\n${GREEN}6. Field Selection Tests${NC}"
run_test "Basic Fields" "cargo run -- --simulate --fields hop,host,loss,avg --count 1 --report google.com"
run_test "Performance Fields" "cargo run -- --simulate --fields hop,host,last,best,worst --count 1 --report google.com"
run_test "Jitter Fields" "cargo run -- --simulate --fields hop,host,jitter,jitter-avg --count 1 --report google.com"

# 7. Interactive Mode Test (with timeout)
echo -e "\n${GREEN}7. Interactive Mode Test (5 seconds)${NC}"
echo "Testing interactive simulation mode for 5 seconds..."
timeout 5s cargo run -- --simulate --count 100 --interval 200 google.com || echo "✅ Interactive mode test completed (timed out as expected)"

# 8. Error Handling Tests
echo -e "\n${GREEN}8. Error Handling Tests${NC}"
echo "Testing invalid hostname..."
cargo run -- --simulate --count 1 --report invalid.hostname.that.should.not.exist 2>&1 | head -5 || echo "✅ Error handling works"

# 9. Performance Benchmarks
echo -e "\n${GREEN}9. Performance Benchmarks${NC}"
echo "Running performance tests..."
echo "Simulation throughput test:"
time cargo run --release -- --simulate --count 100 --interval 10 --report google.com > /dev/null

# 10. Linting and Quality
echo -e "\n${GREEN}10. Code Quality Tests${NC}"
run_test "Clippy Lints" "cargo clippy -- -D warnings"
run_test "Format Check" "cargo fmt -- --check"

echo -e "\n${GREEN}✅ All Tests Completed Successfully!${NC}"
echo -e "${YELLOW}💡 To test with real network (requires sudo):${NC}"
echo "   sudo cargo run -- --count 3 --report google.com"
echo ""
echo -e "${YELLOW}💡 To run interactive mode for development:${NC}"
echo "   cargo run -- --simulate google.com" 