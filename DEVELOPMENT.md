# Development Guide

This guide covers development workflows, version management, and quality assurance for mtr-ng.

## Prerequisites

- Rust 1.70+ with `rustfmt` and `clippy` components
- Git
- Pre-commit (optional but recommended)

## Development Setup

1. **Clone and setup:**
   ```bash
   git clone https://github.com/edejong-dbc/mtr-ng.git  
   cd mtr-ng
   ```

2. **Install pre-commit hooks (recommended):**
   ```bash
   pip install pre-commit
   pre-commit install
   ```

3. **Build and test:**
   ```bash
   cargo build
   cargo test
   ```

## Quality Assurance

### Code Formatting
```bash
# Check formatting
cargo fmt --all -- --check

# Auto-format code
cargo fmt --all
```

### Linting
```bash
# Run clippy with strict linting
cargo clippy --all-targets --all-features -- -D warnings
```

### Testing
```bash
# Run all tests
cargo test --verbose

# Run tests with coverage
cargo install cargo-llvm-cov
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

### Security Audit
```bash
# Install and run security audit
cargo install cargo-audit
cargo audit
```

## Version Management

### Manual Approach
1. Update `Cargo.toml` version
2. Commit and tag manually
3. Push both commit and tag

### Automated Approach (Recommended)
Use the provided version bump script:

```bash
# Patch version (0.1.3 → 0.1.4)
./scripts/bump-version.sh 0.1.4

# Minor version (0.1.4 → 0.2.0)  
./scripts/bump-version.sh 0.2.0

# Major version (0.2.0 → 1.0.0)
./scripts/bump-version.sh 1.0.0
```

The script will:
- ✅ Validate version format
- ✅ Check working directory is clean
- ✅ Update `Cargo.toml`
- ✅ Build to verify changes
- ✅ Create commit and tag
- ✅ Push to trigger release

## CI/CD Pipeline

### Continuous Integration (.github/workflows/ci.yml)
Runs on every push and PR:

- **Code Quality**: `cargo fmt --check` and `cargo clippy`
- **Testing**: Cross-platform tests (Linux, macOS, Windows)
- **Security**: `cargo audit` for vulnerability scanning  
- **Coverage**: Code coverage reporting with codecov

### Release Pipeline (.github/workflows/release.yml)
Triggered by version tags (v*):

- **Version Sync**: Verifies `Cargo.toml` matches git tag
- **Cross-platform Builds**: Linux (x64, ARM64, musl), macOS (Intel, Apple Silicon), Windows
- **Package Generation**: DEB packages for Debian/Ubuntu
- **Distribution**: 
  - GitHub Releases with binaries
  - crates.io publishing
  - Homebrew formula updates

## Release Process

### 1. Prepare Release
```bash
# Ensure all changes are committed
git status

# Run quality checks locally
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

### 2. Version Bump
```bash
# Use the version script (recommended)
./scripts/bump-version.sh 0.1.4

# OR manual process:
# 1. Edit Cargo.toml version
# 2. git add Cargo.toml && git commit -m "Bump version to 0.1.4"
# 3. git tag v0.1.4 && git push origin master && git push origin v0.1.4
```

### 3. Monitor Release
1. Check [GitHub Actions](https://github.com/edejong-dbc/mtr-ng/actions) for build status
2. Verify [GitHub Release](https://github.com/edejong-dbc/mtr-ng/releases) creation
3. Confirm [crates.io](https://crates.io/crates/mtr-ng) publishing
4. Test Homebrew formula update

## Troubleshooting

### Version Mismatch Error
If you see version mismatch errors in CI:

```bash
# Check current versions
grep "^version" Cargo.toml
git tag --sort=-version:refname | head -5

# Fix mismatched version
./scripts/bump-version.sh <correct_version>
```

### Failed CI Checks
- **Formatting**: Run `cargo fmt --all`
- **Linting**: Fix issues reported by `cargo clippy --all-targets --all-features -- -D warnings`
- **Tests**: Debug with `cargo test --verbose`

### Release Build Failures
- Check platform-specific requirements (especially Windows Npcap SDK)
- Verify cross-compilation setup for ARM64 targets
- Review build logs in GitHub Actions

## Adding New Features

1. **Create feature branch**: `git checkout -b feature/new-feature`
2. **Implement with tests**: Write code and corresponding tests
3. **Quality check**: `cargo fmt && cargo clippy && cargo test`
4. **Create PR**: Submit pull request with clear description
5. **CI validation**: Ensure all CI checks pass
6. **Code review**: Address reviewer feedback
7. **Merge**: Squash and merge into main branch

## Performance Optimization

For performance-critical changes:

```bash
# Profile with different optimization levels
cargo build --release
cargo build --profile bench

# Use criterion for benchmarking
cargo install criterion
# Add benchmark tests as needed
```

## Documentation

- Update README.md for user-facing changes
- Add inline documentation for new public APIs
- Update this DEVELOPMENT.md for workflow changes
- Consider adding examples for complex features 