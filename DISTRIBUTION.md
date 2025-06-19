# Distribution Guide for mtr-ng

This document provides comprehensive information for package maintainers and distributors who want to include mtr-ng in their repositories or package managers.

## Package Information

- **Name**: mtr-ng
- **Description**: Modern My Traceroute with real-time network path visualization
- **License**: MIT OR Apache-2.0
- **Homepage**: https://github.com/edejong-dbc/mtr-ng
- **Category**: Network utilities / System administration
- **Requires root**: Yes (for raw socket access)

## Installation Methods

### 1. Quick Install Script
```bash
curl -sSL https://raw.githubusercontent.com/edejong-dbc/mtr-ng/main/install/install.sh | bash
```

### 2. Package Manager Specific

#### Homebrew (macOS)
```bash
# Add our tap
brew tap edejong-dbc/tap

# Install
brew install mtr-ng
```

#### Arch Linux (AUR)
```bash
# Using an AUR helper like yay
yay -S mtr-ng

# Manual installation
git clone https://aur.archlinux.org/mtr-ng.git
cd mtr-ng
makepkg -si
```

#### Debian/Ubuntu
```bash
# Download .deb package from releases
wget https://github.com/edejong-dbc/mtr-ng/releases/download/vX.X.X/mtr-ng_X.X.X_amd64.deb
sudo dpkg -i mtr-ng_X.X.X_amd64.deb
```

#### Fedora/RHEL
```bash
# Download .rpm package from releases (when available)
sudo rpm -i mtr-ng-X.X.X-1.x86_64.rpm
```

#### Cargo (Rust)
```bash
cargo install mtr-ng
```

## Dependencies

### Runtime Dependencies
- `glibc` (Linux) / `libc` (macOS)
- Terminal with Unicode support (recommended)
- Root/administrator privileges for raw socket access

### Build Dependencies
- Rust 1.70+ (MSRV)
- Cargo
- Standard C library and development headers
- pkg-config (Linux)

### Optional Dependencies
- For enhanced terminal support: modern terminal emulator with true color
- For Sixel graphics: terminal with Sixel support (experimental)

## Platform Support

### Tier 1 (Fully Supported)
- `x86_64-unknown-linux-gnu` (Linux x86_64)
- `x86_64-apple-darwin` (macOS Intel)
- `aarch64-apple-darwin` (macOS Apple Silicon)

### Tier 2 (Best Effort)
- `x86_64-unknown-linux-musl` (Alpine Linux)
- `aarch64-unknown-linux-gnu` (Linux ARM64)
- `x86_64-pc-windows-msvc` (Windows)

### Package Manager Files

This repository includes packaging files for various distributions:

- **Homebrew**: `Formula/mtr-ng.rb`
- **Arch Linux**: `packaging/PKGBUILD`
- **Debian**: `packaging/debian/control`
- **Universal**: `install/install.sh`

## Release Process

1. **Version Tagging**: Releases are tagged as `vX.Y.Z` (semantic versioning)
2. **Automated Builds**: GitHub Actions automatically builds for all supported platforms
3. **Package Generation**: Automated generation of .deb, .rpm, and other packages
4. **Distribution**: Binaries and packages uploaded to GitHub Releases

## Security Considerations

**Important**: mtr-ng requires raw socket privileges to function properly. This is necessary for:
- Sending ICMP packets
- Receiving low-level network responses
- Accurate RTT measurements

### Privilege Handling
- The binary should be installed with appropriate permissions
- Consider using capabilities on Linux: `sudo setcap cap_net_raw+ep /usr/bin/mtr-ng`
- On macOS/BSDs: requires `sudo` or running as root
- Windows: requires Administrator privileges

### Installation Recommendations
1. Install binary to standard location (`/usr/bin/` or `/usr/local/bin/`)
2. Set appropriate file permissions (755)
3. Install man page to standard location
4. Include capability setting in post-install scripts where applicable

## Package Verification

Each release includes checksums for verification:
- SHA256 hashes for all binaries
- GPG signatures (when available)
- Reproducible builds support

## Support and Issues

### For Package Maintainers
- Open GitHub issues for packaging-specific problems
- Tag issues with `packaging` label
- Include distribution and version information

### For End Users
- Direct users to the main repository for support
- Include links to documentation and issue tracker
- Provide platform-specific installation instructions

## Quality Assurance

### Testing
Each package should be tested for:
- Binary functionality (`mtr-ng --help`, `mtr-ng --version`)
- Privilege requirements
- Man page installation
- Dependency resolution

### Integration Tests
```bash
# Basic functionality test
mtr-ng --help
mtr-ng --version

# Network test (requires root)
sudo mtr-ng 8.8.8.8 --count 3 --report

# Interactive mode test
sudo mtr-ng google.com
```

## Distribution-Specific Notes

### Debian/Ubuntu
- Package in `net` section
- Priority: optional
- Include debhelper compatibility level 13+
- Follow Debian policy for network utilities

### Arch Linux
- Submit to AUR (Arch User Repository)
- Follow PKGBUILD standards
- Include proper dependency declarations
- Support multiple architectures

### Fedora/RHEL
- Follow RPM packaging guidelines
- Include proper SELinux considerations
- Support EPEL for older RHEL versions

### Alpine Linux
- Use musl-libc build variant
- Minimize dependencies
- Include in community repository

### macOS Homebrew
- Follow Homebrew formula conventions
- Include test suite
- Support both Intel and Apple Silicon

## Contact Information

For packaging questions or distribution requests:
- GitHub Issues: https://github.com/edejong-dbc/mtr-ng/issues
- Email: edejong@fastmail.fm
- Tag: `@edejong-dbc` in related discussions

## License Compliance

mtr-ng is dual-licensed under MIT OR Apache-2.0. When packaging:
- Include LICENSE file in package documentation
- Respect license terms in package metadata
- Follow distribution-specific license handling guidelines

Both licenses are permissive and allow for commercial distribution and modification. 