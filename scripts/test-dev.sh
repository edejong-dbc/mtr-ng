#!/bin/bash
#
# MTR-NG Development Testing Script
# A simple alternative to justfile for basic testing without sudo
#

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"
}

success() {
    echo -e "${GREEN}✅${NC} $1"
}

warning() {
    echo -e "${YELLOW}⚠️${NC} $1"
}

error() {
    echo -e "${RED}❌${NC} $1"
}

# Help function
show_help() {
    cat << EOF
MTR-NG Development Testing Script

Usage: $0 [command]

Available commands:
  test          Run all unit tests (no sudo required)
  test-sim      Quick simulation test in report mode
  test-interactive  Quick simulation test in interactive mode (10s)
  test-protocols    Test all protocols in simulation mode
  test-targets      Test different targets in simulation mode
  test-all      Run comprehensive test suite (no sudo required)
  build         Build the project
  check         Run code quality checks
  clean         Clean build artifacts
  benchmark     Benchmark simulation performance
  help          Show this help message

Examples:
  $0 test-sim                    # Quick smoke test
  $0 test-all                    # Full test suite
  $0 test-real google.com        # Real network test (requires sudo)

For more advanced commands, consider installing 'just':
  cargo install just && just --list
EOF
}

# Function to run unit tests
run_tests() {
    log "Running unit tests..."
    cargo test
    success "Unit tests passed"
}

# Function to run simulation test
test_simulation() {
    log "Running simulation test..."
    cargo run -- --simulate --count 3 --report google.com
    success "Simulation test completed"
}

# Function to run interactive simulation test
test_interactive() {
    log "Running interactive simulation test (10 seconds)..."
    timeout 10s cargo run -- --simulate google.com || true
    success "Interactive simulation test completed"
}

# Function to test all protocols
test_protocols() {
    log "Testing all protocols in simulation mode..."
    
    echo -e "\n${BLUE}Testing ICMP simulation...${NC}"
    cargo run -- --simulate --count 2 --report -P icmp google.com
    
    echo -e "\n${BLUE}Testing UDP simulation...${NC}"
    cargo run -- --simulate --count 2 --report -P udp google.com
    
    echo -e "\n${BLUE}Testing TCP simulation...${NC}"
    cargo run -- --simulate --count 2 --report -P tcp google.com
    
    success "Protocol tests completed"
}

# Function to test different targets
test_targets() {
    log "Testing different targets in simulation mode..."
    
    echo -e "\n${BLUE}Testing localhost...${NC}"
    cargo run -- --simulate --count 2 --report localhost
    
    echo -e "\n${BLUE}Testing IP address...${NC}"
    cargo run -- --simulate --count 2 --report 8.8.8.8
    
    echo -e "\n${BLUE}Testing hostname...${NC}"
    cargo run -- --simulate --count 2 --report example.com
    
    success "Target tests completed"
}

# Function to run comprehensive test suite
test_all() {
    log "Running comprehensive test suite (no sudo required)..."
    
    run_tests
    build_project
    check_code_quality
    test_simulation
    test_protocols
    
    success "All tests passed! 🎉"
}

# Function to build project
build_project() {
    log "Building project..."
    cargo build
    success "Build completed"
}

# Function to check code quality
check_code_quality() {
    log "Running code quality checks..."
    cargo check
    cargo clippy
    cargo fmt --check
    success "Code quality checks passed"
}

# Function to clean build artifacts
clean_build() {
    log "Cleaning build artifacts..."
    cargo clean
    success "Clean completed"
}

# Function to benchmark simulation
benchmark_simulation() {
    log "Benchmarking simulation mode..."
    echo -e "\n${BLUE}Running benchmark...${NC}"
    time cargo run -- --simulate --count 10 --report google.com
    success "Benchmark completed"
}

# Function to run real network test (requires sudo)
test_real() {
    local target=${1:-google.com}
    warning "This requires sudo permissions..."
    log "Running real network test to $target"
    sudo cargo run -- --count 3 --report "$target"
    success "Real network test completed"
}

# Main command handling
case "${1:-}" in
    "test")
        run_tests
        ;;
    "test-sim")
        test_simulation
        ;;
    "test-interactive")
        test_interactive
        ;;
    "test-protocols")
        test_protocols
        ;;
    "test-targets")
        test_targets
        ;;
    "test-all")
        test_all
        ;;
    "build")
        build_project
        ;;
    "check")
        check_code_quality
        ;;
    "clean")
        clean_build
        ;;
    "benchmark")
        benchmark_simulation
        ;;
    "test-real")
        test_real "${2:-google.com}"
        ;;
    "help"|"--help"|"-h")
        show_help
        ;;
    "")
        show_help
        ;;
    *)
        error "Unknown command: $1"
        echo ""
        show_help
        exit 1
        ;;
esac 