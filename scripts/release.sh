#!/bin/bash
set -e

# Release script for glot
# This script handles the release workflow with PR-based merging to main

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default values
LEVEL="${1:-patch}"  # patch, minor, or major

usage() {
    echo "Usage: $0 [patch|minor|major]"
    echo ""
    echo "Examples:"
    echo "  $0 patch   # 0.1.0 -> 0.1.1"
    echo "  $0 minor   # 0.1.0 -> 0.2.0"
    echo "  $0 major   # 0.1.0 -> 1.0.0"
    exit 1
}

# Validate input
if [[ ! "$LEVEL" =~ ^(patch|minor|major)$ ]]; then
    echo -e "${RED}Error: Invalid release level '$LEVEL'${NC}"
    usage
fi

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo -e "${RED}Error: Must be on 'main' branch to release. Currently on '$CURRENT_BRANCH'${NC}"
    exit 1
fi

# Check for uncommitted changes
if ! git diff --quiet || ! git diff --staged --quiet; then
    echo -e "${RED}Error: You have uncommitted changes. Please commit or stash them first.${NC}"
    exit 1
fi

# Make sure we're up to date
echo -e "${YELLOW}Fetching latest changes from origin...${NC}"
git fetch origin main

# Check if we're behind origin/main
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/main)
if [ "$LOCAL" != "$REMOTE" ]; then
    echo -e "${RED}Error: Your local main is not up to date with origin/main.${NC}"
    echo "Please run: git pull origin main"
    exit 1
fi

# Get current version
CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo -e "${GREEN}Current version: $CURRENT_VERSION${NC}"

# Run cargo release in dry-run mode first to get the new version
echo -e "${YELLOW}Running cargo release dry-run...${NC}"
cargo release "$LEVEL" 2>&1 | head -20

# Ask for confirmation
echo ""
read -p "Do you want to proceed with the release? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Release cancelled.${NC}"
    exit 0
fi

# Execute cargo release (creates commit and tag locally)
echo -e "${YELLOW}Creating release commit and tag...${NC}"
cargo release "$LEVEL" --execute --no-confirm

# Get the new version
NEW_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
RELEASE_BRANCH="release/v$NEW_VERSION"
TAG_NAME="v$NEW_VERSION"

echo -e "${GREEN}New version: $NEW_VERSION${NC}"

# Create release branch from current state
echo -e "${YELLOW}Creating release branch '$RELEASE_BRANCH'...${NC}"
git checkout -b "$RELEASE_BRANCH"

# Push the release branch
echo -e "${YELLOW}Pushing release branch...${NC}"
git push -u origin "$RELEASE_BRANCH"

# Create PR using GitHub CLI
echo -e "${YELLOW}Creating Pull Request...${NC}"
PR_URL=$(gh pr create \
    --base main \
    --head "$RELEASE_BRANCH" \
    --title "chore: release v$NEW_VERSION" \
    --body "## Release v$NEW_VERSION

This PR was automatically created by the release script.

### Changes
- Bump version from $CURRENT_VERSION to $NEW_VERSION

### After Merge
After this PR is merged, the tag \`$TAG_NAME\` will be pushed to trigger the release workflow." \
    --label "release" 2>/dev/null || gh pr create \
    --base main \
    --head "$RELEASE_BRANCH" \
    --title "chore: release v$NEW_VERSION" \
    --body "## Release v$NEW_VERSION

This PR was automatically created by the release script.

### Changes
- Bump version from $CURRENT_VERSION to $NEW_VERSION

### After Merge
After this PR is merged, the tag \`$TAG_NAME\` will be pushed to trigger the release workflow.")

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Release PR created successfully!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "PR URL: ${YELLOW}$PR_URL${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Review and merge the PR"
echo "2. After merge, run: ${GREEN}git checkout main && git pull && git push origin $TAG_NAME${NC}"
echo "   This will push the tag and trigger the release workflow."
echo ""
echo -e "${YELLOW}Or switch back to main now:${NC}"
echo "   git checkout main"
