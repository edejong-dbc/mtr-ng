#!/bin/bash
#
# Setup Linux Capabilities for MTR-NG
# This allows running mtr-ng without sudo on Linux systems
#
# WARNING: This grants network raw socket capabilities to the binary.
# Only run this if you understand the security implications.
#

set -e

# Colors for output  
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're on Linux
check_linux() {
    if [[ "$OSTYPE" != "linux-gnu"* ]]; then
        error "This script only works on Linux systems."
        error "Use simulation mode instead: cargo run -- --simulate --count 3 --report google.com"
        exit 1
    fi
}

# Check if setcap is available
check_setcap() {
    if ! command -v setcap &> /dev/null; then
        error "setcap command not found. Please install libcap-utils:"
        echo "  Ubuntu/Debian: sudo apt install libcap2-bin"
        echo "  RHEL/Fedora:   sudo dnf install libcap"
        echo "  Arch Linux:    sudo pacman -S libcap"
        exit 1
    fi
}

# Check if getcap is available  
check_getcap() {
    if ! command -v getcap &> /dev/null; then
        error "getcap command not found. Please install libcap-utils:"
        echo "  Ubuntu/Debian: sudo apt install libcap2-bin"
        echo "  RHEL/Fedora:   sudo dnf install libcap"
        echo "  Arch Linux:    sudo pacman -S libcap"
        exit 1
    fi
}

# Build release binary
build_binary() {
    log "Building release binary..."
    if ! cargo build --release; then
        error "Failed to build binary"
        exit 1
    fi
    
    if [[ ! -f "target/release/mtr-ng" ]]; then
        error "Binary not found at target/release/mtr-ng"
        exit 1
    fi
    
    success "Binary built successfully"
}

# Set capabilities
set_capabilities() {
    local binary_path="target/release/mtr-ng"
    
    log "Setting CAP_NET_RAW capability on $binary_path"
    log "This requires sudo privileges..."
    
    if ! sudo setcap cap_net_raw+ep "$binary_path"; then
        error "Failed to set capabilities"
        exit 1
    fi
    
    success "Capabilities set successfully"
}

# Verify capabilities
verify_capabilities() {
    local binary_path="target/release/mtr-ng"
    
    log "Verifying capabilities..."
    local caps=$(getcap "$binary_path")
    
    if [[ "$caps" == *"cap_net_raw+ep"* ]]; then
        success "Capabilities verified: $caps"
    else
        error "Capabilities not set correctly: $caps"
        exit 1
    fi
}

# Test the binary
test_binary() {
    local binary_path="target/release/mtr-ng"
    
    log "Testing binary without sudo..."
    
    # Test that it works
    if timeout 5s "$binary_path" --count 1 --report google.com &>/dev/null; then
        success "Binary works without sudo!"
    else
        warning "Test didn't complete successfully. Try running manually:"
        echo "  ./target/release/mtr-ng --count 3 --report google.com"
    fi
}

# Show security warning
show_security_warning() {
    cat << 'EOF'

┌─────────────────────────────────────────────────────────────────┐
│                       SECURITY WARNING                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│ This script grants CAP_NET_RAW capability to the mtr-ng binary.│
│ This allows the binary to create raw sockets without sudo.     │
│                                                                 │
│ IMPLICATIONS:                                                   │
│ • The binary can send arbitrary network packets                 │
│ • Anyone who can execute the binary gains this capability      │
│ • This is less secure than using sudo on each run             │
│                                                                 │
│ ALTERNATIVES:                                                   │
│ • Use simulation mode: --simulate (no privileges needed)       │
│ • Use sudo for each run: sudo mtr-ng target                   │
│ • Use containerized testing with Docker                        │
│                                                                 │
│ Only proceed if you understand these security implications.    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

EOF
}

# Main function
main() {
    echo "MTR-NG Linux Capabilities Setup"
    echo "================================"
    echo ""
    
    show_security_warning
    
    read -p "Do you want to proceed? (y/N): " -n 1 -r
    echo ""
    
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        log "Aborted by user"
        echo ""
        echo "Alternative: Use simulation mode (no privileges needed):"
        echo "  cargo run -- --simulate --count 3 --report google.com"
        exit 0
    fi
    
    echo ""
    log "Starting capabilities setup..."
    
    check_linux
    check_setcap  
    check_getcap
    build_binary
    set_capabilities
    verify_capabilities
    test_binary
    
    echo ""
    success "Setup complete! You can now run mtr-ng without sudo:"
    echo ""
    echo "  ./target/release/mtr-ng --count 3 --report google.com"
    echo "  ./target/release/mtr-ng google.com"
    echo ""
    
    warning "Remember: This binary now has raw socket capabilities."
    warning "Consider using simulation mode for development:"
    echo "  cargo run -- --simulate --count 3 --report google.com"
}

# Help function
show_help() {
    cat << 'EOF'
MTR-NG Linux Capabilities Setup

This script sets up Linux capabilities to allow mtr-ng to run without sudo.

Usage:
  ./scripts/setup-capabilities.sh [options]

Options:
  --help, -h    Show this help message

Security Note:
  This grants CAP_NET_RAW capability to the binary, allowing raw socket access.
  Consider using simulation mode instead: --simulate

Examples:
  # Setup capabilities (interactive)
  ./scripts/setup-capabilities.sh

  # After setup, run without sudo
  ./target/release/mtr-ng --count 3 --report google.com

  # Alternative: Use simulation mode (no setup needed)
  cargo run -- --simulate --count 3 --report google.com

EOF
}

# Handle command line arguments
case "${1:-}" in
    "--help"|"-h")
        show_help
        exit 0
        ;;
    "")
        main
        ;;
    *)
        error "Unknown option: $1"
        echo ""
        show_help
        exit 1
        ;;
esac 