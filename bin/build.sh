#!/usr/bin/env bash
# Build the production artifacts: the release backend binary and the portal SPA.
# Kept separate from bin/start.sh so that restarts are instant (start = run only).
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Colors
GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${CYAN}[glove]${NC} $1"; }
ok()   { echo -e "${GREEN}[glove]${NC} $1"; }
fail() { echo -e "${RED}[glove]${NC} $1"; exit 1; }

# Check prerequisites
command -v cargo >/dev/null || fail "cargo not found. Install Rust: https://rustup.rs"
command -v node  >/dev/null || fail "node not found. Install Node.js: https://nodejs.org"
command -v npm   >/dev/null || fail "npm not found."

# Build backend (release)
log "Building backend (release)..."
cargo build --release --quiet
ok "Backend built: target/release/glove"

# Build frontend (reproducible install via npm ci, then static build)
log "Building frontend..."
(cd "$ROOT/portal" && npm ci --silent && npx vite build --outDir "$ROOT/target/portal" --emptyOutDir)
ok "Frontend built: target/portal"

echo ""
ok "Build complete. Run bin/start.sh to launch."
