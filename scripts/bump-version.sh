#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Show usage
usage() {
    echo "Usage: $0 <new_version>"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.4     # Patch version bump"
    echo "  $0 0.2.0     # Minor version bump"
    echo "  $0 1.0.0     # Major version bump"
    echo ""
    echo "This script will:"
    echo "  1. Update Cargo.toml version"
    echo "  2. Create a git commit"
    echo "  3. Create a git tag"
    echo "  4. Push both commit and tag"
    exit 1
}

# Check if version argument provided
if [ $# -ne 1 ]; then
    usage
fi

NEW_VERSION=$1

# Validate version format (basic semver check)
if ! [[ $NEW_VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo -e "${RED}❌ Invalid version format. Use semantic versioning (e.g., 0.1.4)${NC}"
    exit 1
fi

# Check if we're in a git repository
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo -e "${RED}❌ Not in a git repository${NC}"
    exit 1
fi

# Check if working directory is clean
if ! git diff-index --quiet HEAD --; then
    echo -e "${YELLOW}⚠️  Working directory has uncommitted changes${NC}"
    echo "Please commit or stash your changes before bumping version."
    exit 1
fi

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

echo -e "${BLUE}📦 Version Bump${NC}"
echo "Current version: $CURRENT_VERSION"
echo "New version:     $NEW_VERSION"
echo ""

# Confirm the change
read -p "Continue with version bump? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
fi

# Update Cargo.toml
echo -e "${YELLOW}🔄 Updating Cargo.toml...${NC}"
sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm Cargo.toml.bak

# Verify the change
NEW_CARGO_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
if [ "$NEW_CARGO_VERSION" != "$NEW_VERSION" ]; then
    echo -e "${RED}❌ Failed to update Cargo.toml version${NC}"
    exit 1
fi

# Build to verify everything still works
echo -e "${YELLOW}🔨 Building to verify...${NC}"
if ! cargo build --release; then
    echo -e "${RED}❌ Build failed after version update${NC}"
    echo "Please fix the issues and try again."
    exit 1
fi

# Commit the version change
echo -e "${YELLOW}📝 Creating commit...${NC}"
git add Cargo.toml
git commit -m "Bump version to $NEW_VERSION"

# Create and push tag
echo -e "${YELLOW}🏷️  Creating and pushing tag...${NC}"
git tag "v$NEW_VERSION"

# Push both commit and tag
echo -e "${YELLOW}⬆️  Pushing to remote...${NC}"
git push origin $(git branch --show-current)
git push origin "v$NEW_VERSION"

echo ""
echo -e "${GREEN}✅ Version successfully bumped to $NEW_VERSION${NC}"
echo "🚀 GitHub Actions will now build and release version v$NEW_VERSION"
echo ""
echo "Next steps:"
echo "  - Check GitHub Actions progress: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git/\1/')/actions"
echo "  - Monitor release creation: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git/\1/')/releases" 