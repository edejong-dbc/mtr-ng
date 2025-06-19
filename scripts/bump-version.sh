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
    echo "  2. Create a git commit and push"
    echo "  3. Wait for CI to pass"
    echo "  4. Create and push git tag (triggers release)"
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

# Push commit first to trigger CI
echo -e "${YELLOW}⬆️  Pushing commit to trigger CI...${NC}"
git push origin $(git branch --show-current)

# Wait for CI to pass
echo -e "${YELLOW}⏳ Waiting for CI to complete...${NC}"
wait_for_ci() {
    local repo_info=$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git/\1/')
    local commit_sha=$(git rev-parse HEAD)
    local max_attempts=60  # 10 minutes (60 * 10 seconds)
    local attempt=0
    
    echo "Monitoring CI status for commit: $commit_sha"
    echo "Repository: $repo_info"
    echo ""
    
    while [ $attempt -lt $max_attempts ]; do
        # Check if gh CLI is available
        if command -v gh > /dev/null 2>&1; then
            # Use GitHub CLI to check status
            local status=$(gh run list --commit "$commit_sha" --json status --jq '.[0].status' 2>/dev/null || echo "unknown")
            local conclusion=$(gh run list --commit "$commit_sha" --json conclusion --jq '.[0].conclusion' 2>/dev/null || echo "unknown")
            
            case "$status" in
                "completed")
                    if [ "$conclusion" = "success" ]; then
                        echo -e "${GREEN}✅ CI passed! Proceeding with release...${NC}"
                        return 0
                    elif [ "$conclusion" = "failure" ] || [ "$conclusion" = "cancelled" ]; then
                        echo -e "${RED}❌ CI failed with status: $conclusion${NC}"
                        echo "Please fix the issues and try again."
                        echo "CI Results: https://github.com/$repo_info/actions"
                        return 1
                    fi
                    ;;
                "in_progress"|"queued")
                    echo -e "${BLUE}🔄 CI in progress... (attempt $((attempt + 1))/$max_attempts)${NC}"
                    ;;
                *)
                    echo -e "${YELLOW}⚠️  Unknown CI status: $status (attempt $((attempt + 1))/$max_attempts)${NC}"
                    ;;
            esac
        else
            echo -e "${YELLOW}⚠️  GitHub CLI not found. Install 'gh' for automatic CI monitoring.${NC}"
            echo "Please manually verify CI passes before continuing."
            echo "CI Status: https://github.com/$repo_info/actions"
            echo ""
            read -p "Has CI passed? Continue with release? (y/N): " -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                return 0
            else
                echo "Release cancelled."
                return 1
            fi
        fi
        
        sleep 10
        attempt=$((attempt + 1))
    done
    
    echo -e "${RED}❌ Timeout waiting for CI to complete${NC}"
    echo "Please check CI status manually: https://github.com/$repo_info/actions"
    return 1
}

if ! wait_for_ci; then
    echo -e "${RED}❌ Cannot proceed with release - CI checks failed or timed out${NC}"
    exit 1
fi

# Create and push tag only after CI passes
echo -e "${YELLOW}🏷️  Creating and pushing release tag...${NC}"
git tag "v$NEW_VERSION"
git push origin "v$NEW_VERSION"

echo ""
echo -e "${GREEN}✅ Version successfully bumped to $NEW_VERSION${NC}"
echo -e "${GREEN}✅ CI checks passed - release is now building${NC}"
echo "🚀 GitHub Actions will now build and release version v$NEW_VERSION"
echo ""
echo "Next steps:"
echo "  - Monitor release build: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git/\1/')/actions"
echo "  - Check release when ready: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git/\1/')/releases" 