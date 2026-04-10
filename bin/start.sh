#!/usr/bin/env bash
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

cleanup() {
    log "Shutting down..."
    [ -n "$BACKEND_PID" ] && kill "$BACKEND_PID" 2>/dev/null
    [ -n "$FRONTEND_PID" ] && kill "$FRONTEND_PID" 2>/dev/null
    wait 2>/dev/null
    ok "Stopped."
}
trap cleanup EXIT INT TERM

# Parse options
DEV_MODE=false
if [ "$1" = "--dev" ]; then
    DEV_MODE=true
fi

# Check prerequisites
command -v cargo >/dev/null || fail "cargo not found. Install Rust: https://rustup.rs"
command -v node  >/dev/null || fail "node not found. Install Node.js: https://nodejs.org"
command -v npm   >/dev/null || fail "npm not found."

# Check GTFS data
[ -f "$ROOT/data/gtfs/stop_times.txt" ] || fail "GTFS data not found in data/gtfs/. Run bin/download.sh first."

# Start Valhalla if not already running
if ! "$ROOT/bin/valhalla.sh" status 2>/dev/null | grep -q "running"; then
    log "Valhalla is not running, starting it..."
    "$ROOT/bin/valhalla.sh" start
else
    ok "Valhalla is already running."
fi

if [ "$DEV_MODE" = true ]; then
    # --- DEV MODE (hot-reload) ---
    log "Starting in DEV mode (hot-reload)..."

    command -v cargo-watch >/dev/null || fail "cargo-watch not found. Install it: cargo install cargo-watch"

    # Start backend with cargo-watch for hot-reload
    log "Starting backend (cargo-watch)..."
    cargo watch -x run -w src -w config.yaml -q &
    BACKEND_PID=$!

    # Install frontend dependencies
    (cd "$ROOT/portal" && npm install --silent)

    # Start frontend with Vite dev server (HMR)
    log "Starting frontend (vite dev)..."
    (cd "$ROOT/portal" && npx vite --host) &
    FRONTEND_PID=$!
    sleep 2

    echo ""
    ok "==========================="
    ok "  Glove DEV mode"
    ok "  Frontend: http://localhost:3000 (HMR)"
    ok "  API:      http://localhost:8080 (hot-reload)"
    ok "==========================="
    echo ""
    log "Press Ctrl+C to stop."

else
    # --- PROD MODE ---
    log "Starting in PROD mode..."

    # Build backend
    log "Building backend..."
    cargo build --release --quiet
    ok "Backend built."

    # Install & build frontend
    log "Building frontend..."
    (cd "$ROOT/portal" && npm install --silent && npx vite build --outDir "$ROOT/target/portal" --emptyOutDir)
    ok "Frontend built."

    # Start backend
    log "Starting backend..."
    "$ROOT/target/release/glove" &
    BACKEND_PID=$!

    # Start frontend static server
    log "Starting frontend..."
    npx serve "$ROOT/target/portal" -l 3000 -s &
    FRONTEND_PID=$!

    # Wait for backend to be ready
    for i in $(seq 1 60); do
        sleep 1
        if curl -sf -o /dev/null "http://localhost:8080/api/status" 2>/dev/null; then
            ok "Backend ready (PID $BACKEND_PID)"
            break
        fi
        if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
            fail "Backend failed to start."
        fi
    done

    echo ""
    ok "==========================="
    ok "  Glove is running"
    ok "  Frontend: http://localhost:3000"
    ok "  API:      http://localhost:8080"
    ok "==========================="
    echo ""
    log "Press Ctrl+C to stop."
fi

wait
