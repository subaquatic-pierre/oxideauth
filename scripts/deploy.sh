#!/usr/bin/env bash
#
# deploy.sh — Build the Rust API binary in release mode, tag the commit,
# and push the tag.  Build-only — no deployment step, no test step.
#
# Usage:
#   ./scripts/deploy.sh               # auto-increment patch version
#   ./scripts/deploy.sh minor         # bump minor version
#   ./scripts/deploy.sh major         # bump major version
#   ./scripts/deploy.sh X.Y.Z         # use an explicit version tag
#   ./scripts/deploy.sh -y patch      # skip confirmation prompt

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

# ── helpers ────────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { printf "${GREEN}[deploy]${NC} %s\n" "$*"; }
warn() { printf "${YELLOW}[deploy] WARNING:${NC} %s\n" "$*" >&2; }
err()  { printf "${RED}[deploy] ERROR:${NC} %s\n" "$*" >&2; exit 1; }

# ── pre-flight checks ─────────────────────────────────────────────────────

command -v cargo >/dev/null 2>&1 || err "cargo is required but not found."
command -v git   >/dev/null 2>&1 || err "git is required but not found."

if ! git diff-index --quiet HEAD --; then
  warn "Working tree is dirty. Commit or stash changes before deploying."
  exit 1
fi

CURRENT_BRANCH="$(git branch --show-current)"
if [ -z "$CURRENT_BRANCH" ]; then
  err "Not on any branch (detached HEAD). Checkout a branch first."
fi
if [ "$CURRENT_BRANCH" != "main" ]; then
  err "Must be on 'main' branch. Current branch: ${CURRENT_BRANCH}"
fi
log "Current branch: ${CURRENT_BRANCH}"

# ── helpers: version manipulation ─────────────────────────────────────────

# Read the current version from Cargo.toml [package] section.
current_version() {
  grep -m1 '^version = "' Cargo.toml | sed 's/^version = "\(.*\)"/\1/'
}

# Best-effort fallback: latest git tag (without leading 'v').
latest_tag() {
  git describe --tags --abbrev=0 2>/dev/null | sed 's/^v//' || echo "0.0.0"
}

# Check whether cargo-edit (cargo set-version) is available.
HAS_CARGO_EDIT=false
cargo set-version --help >/dev/null 2>&1 && HAS_CARGO_EDIT=true || true

# Bump Cargo.toml version using cargo-edit or plain sed.
bump_version() {
  local level="$1"   # major | minor | patch
  if [ "$HAS_CARGO_EDIT" = true ]; then
    cargo set-version --bump "$level"
  else
    local base cur_major cur_minor cur_patch newver
    base="$(current_version)"
    if [ -z "$base" ]; then
      base="$(latest_tag)"
    fi
    cur_major="${base%%.*}"
    cur_minor="${base#*.}"; cur_minor="${cur_minor%%.*}"
    cur_patch="${base##*.}"
    case "$level" in
      major) cur_major=$((cur_major + 1)); cur_minor=0; cur_patch=0 ;;
      minor) cur_minor=$((cur_minor + 1)); cur_patch=0 ;;
      patch) cur_patch=$((cur_patch + 1)) ;;
    esac
    newver="${cur_major}.${cur_minor}.${cur_patch}"
    sed -i 's/^version = ".*"/version = "'"${newver}"'"/' Cargo.toml
  fi
}

# Fetch initial version for logging (before any bump).
CURRENT="$(current_version)"
if [ -z "$CURRENT" ]; then
  CURRENT="$(latest_tag)"
fi
CURRENT="${CURRENT}"
log "Current version: ${CURRENT}"

# ── parse arguments ───────────────────────────────────────────────────────

YES=false

while [ $# -gt 0 ]; do
  case "$1" in
    -y|--yes) YES=true; shift ;;
    *)        break ;;
  esac
done

TAG=""
BUMPED=false

if [ $# -eq 0 ]; then
  # No args: bump patch version.
  bump_version patch
  BUMPED=true
  TAG="$(current_version)"
elif [ $# -eq 1 ]; then
  case "$1" in
    major|minor|patch)
      bump_version "$1"
      BUMPED=true
      TAG="$(current_version)"
      ;;
    *)
      TAG="$1"
      ;;
  esac
else
  err "Too many arguments. Usage: ./scripts/deploy.sh [-y] [major|minor|patch|X.Y.Z]"
fi

if [ -z "$TAG" ]; then
  err "Could not determine a version tag."
fi

log "Deployment tag will be: ${TAG}"

# ── confirm ───────────────────────────────────────────────────────────────

if [ "${YES}" = false ]; then
  read -r -p "$(printf "${YELLOW}Proceed with deploy tag ${TAG}?${NC} [y/N] ")" CONFIRM </dev/tty
  if [ "${CONFIRM}" != "y" ] && [ "${CONFIRM}" != "Y" ]; then
    log "Aborted."
    exit 0
  fi
fi

# ── build ─────────────────────────────────────────────────────────────────

log "Building release binary..."
cargo build --release

log "Build completed successfully."

# ── tag ───────────────────────────────────────────────────────────────────

log "Creating tag ${TAG}..."
git tag -a "${TAG}" -m "deploy: ${TAG}" 2>/dev/null || {
  warn "Tag ${TAG} already exists locally. Skipping tag creation."
}

log "Pushing tag ${TAG} to origin..."
git push origin "${TAG}"

# ── commit version bump if Cargo.toml was modified ────────────────────────

if [ "${BUMPED}" = true ]; then
  if ! git diff --quiet Cargo.toml Cargo.lock 2>/dev/null; then
    log "Committing version bump..."
    git add Cargo.toml Cargo.lock
    git commit -m "chore: bump version to ${TAG}"
    git push origin "${CURRENT_BRANCH}"
  fi
fi

log "Done."
