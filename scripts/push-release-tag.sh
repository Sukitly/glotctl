#!/bin/bash
set -e

# Push release tag after PR is merged
# This script should be run after the release PR is merged to main

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo -e "${RED}Error: Must be on 'main' branch. Currently on '$CURRENT_BRANCH'${NC}"
    echo "Run: git checkout main && git pull"
    exit 1
fi

# Make sure we're up to date
echo -e "${YELLOW}Pulling latest changes...${NC}"
git pull origin main

# Get the current version from Cargo.toml
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
TAG_NAME="v$VERSION"

# Check if the tag exists locally
if ! git tag -l | grep -q "^$TAG_NAME$"; then
    echo -e "${RED}Error: Tag '$TAG_NAME' does not exist locally.${NC}"
    echo "Make sure the release PR was created properly."
    exit 1
fi

# Check if tag already exists on remote
if git ls-remote --tags origin | grep -q "refs/tags/$TAG_NAME"; then
    echo -e "${YELLOW}Tag '$TAG_NAME' already exists on remote.${NC}"
    exit 0
fi

# Push the tag
echo -e "${YELLOW}Pushing tag '$TAG_NAME' to origin...${NC}"
git push origin "$TAG_NAME"

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Tag $TAG_NAME pushed successfully!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "The release workflow should now be triggered."
echo "Check: https://github.com/Sukitly/glotctl/actions"

# Clean up release branch if it exists
RELEASE_BRANCH="release/$TAG_NAME"
if git branch -r | grep -q "origin/$RELEASE_BRANCH"; then
    echo ""
    read -p "Delete remote release branch '$RELEASE_BRANCH'? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        git push origin --delete "$RELEASE_BRANCH" 2>/dev/null || true
        git branch -d "$RELEASE_BRANCH" 2>/dev/null || true
        echo -e "${GREEN}Release branch cleaned up.${NC}"
    fi
fi
