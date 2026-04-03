#!/usr/bin/env bash
set -e

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

# Colors
GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${CYAN}[glove]${NC} $1"; }
ok()   { echo -e "${GREEN}[glove]${NC} $1"; }
fail() { echo -e "${RED}[glove]${NC} $1"; exit 1; }

cleanup() {
    log "Shutting down..."
    [ -n "$BACKEND_PID" ] && kill "$BACKEND_PID" 2>/dev/null
    [ -n "$FRONTEND_PID" ] && kill "$FRONTEND_PID" 2>/dev/null
    wait 2>/dev/null
    ok "Stopped."
}
trap cleanup EXIT INT TERM

# Check prerequisites
command -v cargo >/dev/null || fail "cargo not found. Install Rust: https://rustup.rs"
command -v node  >/dev/null || fail "node not found. Install Node.js: https://nodejs.org"
command -v npm   >/dev/null || fail "npm not found."

# Check GTFS data
[ -f "$ROOT/data/stop_times.txt" ] || fail "GTFS data not found in data/. Place GTFS files there first."

# Build backend
log "Building backend..."
cargo build --release --quiet
ok "Backend built."

# Install & build frontend
log "Installing frontend dependencies..."
(cd "$ROOT/portal" && npm install --silent)
ok "Frontend ready."

# Start backend
log "Starting backend..."
"$ROOT/target/release/glove" &
BACKEND_PID=$!

# Wait for backend to be ready
for i in $(seq 1 60); do
    sleep 1
    if curl -sf -o /dev/null "http://localhost:8080/api/journeys?from=test&to=test" 2>/dev/null; then
        ok "Backend ready (PID $BACKEND_PID)"
        break
    fi
    if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
        fail "Backend failed to start."
    fi
done

# Start frontend
log "Starting frontend..."
(cd "$ROOT/portal" && npx vite --host) &
FRONTEND_PID=$!
sleep 2

echo ""
ok "==========================="
ok "  Glove is running"
ok "  Frontend: http://localhost:3000"
ok "  API:      http://localhost:8080"
ok "==========================="
echo ""
log "Press Ctrl+C to stop."

wait
