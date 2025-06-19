#!/bin/bash
# Installation script for mtr-ng
# Supports multiple installation methods and package managers

set -e

REPO_URL="https://github.com/edejong-dbc/mtr-ng"
VERSION="latest"
INSTALL_DIR="/usr/local/bin"
MAN_DIR="/usr/local/share/man/man1"
DOC_DIR="/usr/local/share/doc/mtr-ng"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

# Check if running as root
check_privileges() {
    if [[ $EUID -eq 0 ]]; then
        INSTALL_DIR="/usr/bin"
        MAN_DIR="/usr/share/man/man1"
        DOC_DIR="/usr/share/doc/mtr-ng"
    else
        warn "Not running as root. Installing to user directories."
        INSTALL_DIR="$HOME/.local/bin"
        MAN_DIR="$HOME/.local/share/man/man1"
        DOC_DIR="$HOME/.local/share/doc/mtr-ng"
        
        # Create directories if they don't exist
        mkdir -p "$INSTALL_DIR" "$MAN_DIR" "$DOC_DIR"
        
        # Add to PATH if not already there
        if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
            warn "Add $INSTALL_DIR to your PATH by adding this to your shell profile:"
            echo "    export PATH=\"$INSTALL_DIR:\$PATH\""
        fi
    fi
}

# Detect system architecture
detect_arch() {
    local arch
    arch=$(uname -m)
    case $arch in
        x86_64|amd64)
            echo "x86_64"
            ;;
        aarch64|arm64)
            echo "aarch64"
            ;;
        armv7l)
            echo "armv7"
            ;;
        *)
            error "Unsupported architecture: $arch"
            exit 1
            ;;
    esac
}

# Detect operating system
detect_os() {
    local os
    os=$(uname -s)
    case $os in
        Linux)
            echo "linux"
            ;;
        Darwin)
            echo "macos"
            ;;
        MINGW*|CYGWIN*|MSYS*)
            echo "windows"
            ;;
        *)
            error "Unsupported operating system: $os"
            exit 1
            ;;
    esac
}

# Check for package manager installation
check_package_managers() {
    if command -v cargo >/dev/null 2>&1; then
        log "Found Rust/Cargo - can install from source"
        return 0
    fi
    
    # Check for various package managers
    if command -v brew >/dev/null 2>&1; then
        log "Found Homebrew"
        return 0
    fi
    
    if command -v apt >/dev/null 2>&1; then
        log "Found APT (Debian/Ubuntu)"
        return 0
    fi
    
    if command -v yum >/dev/null 2>&1; then
        log "Found YUM (RHEL/CentOS)"
        return 0
    fi
    
    if command -v dnf >/dev/null 2>&1; then
        log "Found DNF (Fedora)"
        return 0
    fi
    
    if command -v pacman >/dev/null 2>&1; then
        log "Found Pacman (Arch Linux)"
        return 0
    fi
    
    warn "No supported package manager found. Will attempt binary installation."
    return 1
}

# Install via Homebrew
install_homebrew() {
    log "Installing mtr-ng via Homebrew..."
    if brew install mtr-ng 2>/dev/null; then
        success "Installed mtr-ng via Homebrew"
        return 0
    else
        warn "Homebrew installation failed or formula not available"
        return 1
    fi
}

# Install via Cargo
install_cargo() {
    log "Installing mtr-ng via Cargo from source..."
    if cargo install --git "$REPO_URL" --locked; then
        success "Installed mtr-ng via Cargo"
        return 0
    else
        error "Cargo installation failed"
        return 1
    fi
}

# Install binary release
install_binary() {
    local os arch binary_url temp_dir
    
    os=$(detect_os)
    arch=$(detect_arch)
    
    log "Installing pre-built binary for $os-$arch..."
    
    # Create temporary directory
    temp_dir=$(mktemp -d)
    cd "$temp_dir"
    
    # Download the appropriate binary
    binary_url="$REPO_URL/releases/download/$VERSION/mtr-ng-$os-$arch"
    if [[ "$os" == "windows" ]]; then
        binary_url="${binary_url}.exe"
    fi
    
    log "Downloading from: $binary_url"
    if ! curl -L -o mtr-ng "$binary_url"; then
        error "Failed to download binary"
        rm -rf "$temp_dir"
        return 1
    fi
    
    # Make executable and install
    chmod +x mtr-ng
    sudo cp mtr-ng "$INSTALL_DIR/"
    
    # Download and install man page
    if curl -L -o mtr-ng.1 "$REPO_URL/raw/main/install/mtr-ng.1" 2>/dev/null; then
        sudo mkdir -p "$MAN_DIR"
        sudo cp mtr-ng.1 "$MAN_DIR/"
        sudo gzip -f "$MAN_DIR/mtr-ng.1"
    fi
    
    # Clean up
    rm -rf "$temp_dir"
    success "Installed mtr-ng binary to $INSTALL_DIR"
    return 0
}

# Build from source
build_from_source() {
    local temp_dir
    
    log "Building mtr-ng from source..."
    
    # Check for Rust
    if ! command -v cargo >/dev/null 2>&1; then
        error "Rust/Cargo not found. Install from: https://rustup.rs/"
        return 1
    fi
    
    # Create temporary directory
    temp_dir=$(mktemp -d)
    cd "$temp_dir"
    
    # Clone repository
    log "Cloning repository..."
    git clone "$REPO_URL" .
    
    # Build release binary
    log "Building release binary..."
    cargo build --release
    
    # Install binary
    sudo cp target/release/mtr-ng "$INSTALL_DIR/"
    
    # Install man page
    sudo mkdir -p "$MAN_DIR" "$DOC_DIR"
    sudo cp install/mtr-ng.1 "$MAN_DIR/"
    sudo gzip -f "$MAN_DIR/mtr-ng.1"
    
    # Install documentation
    sudo cp README.md LICENSE "$DOC_DIR/"
    
    # Clean up
    cd /
    rm -rf "$temp_dir"
    
    success "Built and installed mtr-ng from source"
    return 0
}

# Main installation logic
main() {
    log "Starting mtr-ng installation..."
    
    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --version)
                VERSION="$2"
                shift 2
                ;;
            --source)
                log "Forcing source installation"
                build_from_source
                exit $?
                ;;
            --binary)
                log "Forcing binary installation"
                install_binary
                exit $?
                ;;
            --help)
                echo "Usage: $0 [OPTIONS]"
                echo "Options:"
                echo "  --version VERSION  Install specific version (default: latest)"
                echo "  --source          Force installation from source"
                echo "  --binary          Force binary installation"
                echo "  --help            Show this help"
                exit 0
                ;;
            *)
                error "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    check_privileges
    
    # Try installation methods in order of preference
    if command -v brew >/dev/null 2>&1 && [[ "$(detect_os)" == "macos" ]]; then
        if install_homebrew; then
            exit 0
        fi
    fi
    
    if command -v cargo >/dev/null 2>&1; then
        if install_cargo; then
            exit 0
        fi
    fi
    
    if install_binary; then
        exit 0
    fi
    
    if build_from_source; then
        exit 0
    fi
    
    error "All installation methods failed"
    exit 1
}

# Run main function
main "$@" 