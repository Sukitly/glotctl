#!/bin/bash
set -e

# Release script for glot
# This script creates a release PR. After the PR is merged,
# GitHub Actions will automatically create a tag and trigger the release workflow.

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

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo -e "${GREEN}Current version: $CURRENT_VERSION${NC}"

# Parse version components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

# Calculate new version
case "$LEVEL" in
    patch)
        NEW_PATCH=$((PATCH + 1))
        NEW_VERSION="$MAJOR.$MINOR.$NEW_PATCH"
        ;;
    minor)
        NEW_MINOR=$((MINOR + 1))
        NEW_VERSION="$MAJOR.$NEW_MINOR.0"
        ;;
    major)
        NEW_MAJOR=$((MAJOR + 1))
        NEW_VERSION="$NEW_MAJOR.0.0"
        ;;
esac

RELEASE_BRANCH="release/v$NEW_VERSION"
TAG_NAME="v$NEW_VERSION"

echo -e "${GREEN}New version: $NEW_VERSION${NC}"
echo ""

# Check if release branch already exists
if git show-ref --verify --quiet "refs/heads/$RELEASE_BRANCH" || \
   git show-ref --verify --quiet "refs/remotes/origin/$RELEASE_BRANCH"; then
    echo -e "${RED}Error: Branch '$RELEASE_BRANCH' already exists.${NC}"
    exit 1
fi

# Check if tag already exists
if git tag -l | grep -q "^$TAG_NAME$" || \
   git ls-remote --tags origin | grep -q "refs/tags/$TAG_NAME"; then
    echo -e "${RED}Error: Tag '$TAG_NAME' already exists.${NC}"
    exit 1
fi

# Ask for confirmation
read -p "Create release PR for v$NEW_VERSION? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Release cancelled.${NC}"
    exit 0
fi

# Update version in Cargo.toml
echo -e "${YELLOW}Updating Cargo.toml...${NC}"
sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update Cargo.lock
echo -e "${YELLOW}Updating Cargo.lock...${NC}"
cargo check --quiet

# Create release branch
echo -e "${YELLOW}Creating release branch '$RELEASE_BRANCH'...${NC}"
git checkout -b "$RELEASE_BRANCH"

# Commit changes
echo -e "${YELLOW}Committing changes...${NC}"
git add Cargo.toml Cargo.lock
git commit -m "chore: release v$NEW_VERSION"

# Push the release branch
echo -e "${YELLOW}Pushing release branch...${NC}"
git push -u origin "$RELEASE_BRANCH"

# Create PR using GitHub CLI
echo -e "${YELLOW}Creating Pull Request...${NC}"
PR_BODY="## Release v$NEW_VERSION

This PR was automatically created by the release script.

### Changes
- Bump version from $CURRENT_VERSION to $NEW_VERSION

### What happens after merge
After this PR is merged, GitHub Actions will automatically:
1. Create tag \`$TAG_NAME\` on the merged commit
2. Trigger the release workflow to build and publish binaries"

PR_URL=$(gh pr create \
    --base main \
    --head "$RELEASE_BRANCH" \
    --title "chore: release v$NEW_VERSION" \
    --body "$PR_BODY")

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Release PR created successfully!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "PR URL: ${YELLOW}$PR_URL${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Review and merge the PR"
echo "2. GitHub Actions will automatically create the tag and trigger the release"
echo ""
echo -e "${YELLOW}Switching back to main branch...${NC}"
git checkout main
