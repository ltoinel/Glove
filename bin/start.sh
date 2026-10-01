#!/usr/bin/env bash
# Start Glove behind Caddy, on two HTTPS domains:
#
#   https://portal.glove   the React portal
#   https://api.glove      the REST API (localhost:8080)
#
# Usage: bin/start.sh [--dev | --docker]
#   (default)  run the release artifacts from bin/build.sh (built if missing)
#   --dev      cargo-watch backend + Vite dev server with HMR
#   --docker   build and run the api/portal/valhalla images (docker/docker-compose.yml)
#
# Override the names with GLOVE_PORTAL_HOST / GLOVE_API_HOST, and the ports with
# GLOVE_HTTPS_PORT / GLOVE_HTTP_PORT (e.g. 8443/8880, no privileges needed).
# Outside --docker, the portal origin must also be listed in
# `server.cors_origins` (config.yaml).
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# shellcheck source=lib/ensure-config.sh
. "$ROOT/bin/lib/ensure-config.sh"
ensure_config "$ROOT"

PORTAL_HOST="${GLOVE_PORTAL_HOST:-portal.glove}"
API_HOST="${GLOVE_API_HOST:-api.glove}"
HTTPS_PORT="${GLOVE_HTTPS_PORT:-443}"
HTTP_PORT="${GLOVE_HTTP_PORT:-80}"
PORT_SUFFIX=""
[ "$HTTPS_PORT" != "443" ] && PORT_SUFFIX=":$HTTPS_PORT"
PORTAL_URL="https://$PORTAL_HOST$PORT_SUFFIX"
API_URL="https://$API_HOST$PORT_SUFFIX"
COMPOSE_FILE="$ROOT/docker/docker-compose.yml"

# Colors
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${CYAN}[glove]${NC} $1"; }
ok()   { echo -e "${GREEN}[glove]${NC} $1"; }
warn() { echo -e "${YELLOW}[glove]${NC} $1"; }
fail() { echo -e "${RED}[glove]${NC} $1"; exit 1; }

cleanup() {
    # Ctrl+C fires INT, then EXIT: shut down once.
    trap - EXIT INT TERM
    log "Shutting down..."
    [ -n "$CADDY_PID" ] && kill "$CADDY_PID" 2>/dev/null
    [ -n "$BACKEND_PID" ] && kill "$BACKEND_PID" 2>/dev/null
    [ -n "$FRONTEND_PID" ] && kill "$FRONTEND_PID" 2>/dev/null
    [ -n "$LOGS_PID" ] && kill "$LOGS_PID" 2>/dev/null
    [ "$COMPOSE_STARTED" = true ] && $DOCKER compose -f "$COMPOSE_FILE" down
    wait 2>/dev/null
    ok "Stopped."
}
trap cleanup EXIT INT TERM

# Caddy listens on 80 (HTTP -> HTTPS redirect) and 443 by default. Both ports
# are privileged, and the Debian package's own service already holds them.
check_caddy_ports() {
    local unprivileged
    unprivileged="$(cat /proc/sys/net/ipv4/ip_unprivileged_port_start 2>/dev/null || echo 1024)"
    if [ "$HTTP_PORT" -ge "$unprivileged" ] && [ "$HTTPS_PORT" -ge "$unprivileged" ]; then
        return
    fi
    if systemctl is-active --quiet caddy 2>/dev/null; then
        fail "The system Caddy service holds ports 80/443. Disable it once with:
    sudo systemctl disable --now caddy"
    fi
    if [ "$(id -u)" -ne 0 ] \
        && ! getcap "$(command -v caddy)" 2>/dev/null | grep -q cap_net_bind_service; then
        fail "Caddy cannot bind ports 80/443 as $(whoami). Allow it once with:
    sudo setcap cap_net_bind_service=+ep $(command -v caddy)
(to redo after each caddy package upgrade)"
    fi
}

# The names are not public domains: they must resolve locally.
check_hosts() {
    local missing=()
    for host in "$PORTAL_HOST" "$API_HOST"; do
        getent hosts "$host" >/dev/null || missing+=("$host")
    done
    [ ${#missing[@]} -eq 0 ] && return
    warn "Not resolvable here: ${missing[*]}. Add to /etc/hosts:"
    warn "    127.0.0.1 $PORTAL_HOST $API_HOST"
    if grep -qi microsoft /proc/version 2>/dev/null; then
        warn "Under WSL, a Windows browser reads C:\\Windows\\System32\\drivers\\etc\\hosts instead."
    fi
}

check_cors() {
    grep -q "\"$PORTAL_URL\"" "$ROOT/config.yaml" \
        || warn "config.yaml server.cors_origins lacks \"$PORTAL_URL\": the portal's API calls will be refused."
}

check_prerequisites() {
    command -v caddy >/dev/null || fail "caddy not found. Install it: https://caddyserver.com/docs/install"
    if [ "$MODE" = docker ]; then
        command -v docker >/dev/null || fail "docker not found. Install Docker first."
        # Same fallback as bin/valhalla.sh when the user is not in the docker group.
        DOCKER="docker"
        docker info >/dev/null 2>&1 || DOCKER="sudo docker"
        $DOCKER compose version >/dev/null 2>&1 || fail "docker compose plugin not found."
    else
        command -v cargo >/dev/null || fail "cargo not found. Install Rust: https://rustup.rs"
        command -v node  >/dev/null || fail "node not found. Install Node.js: https://nodejs.org"
        command -v npm   >/dev/null || fail "npm not found."
        check_cors
    fi
    check_caddy_ports
    check_hosts
    [ -f "$ROOT/data/gtfs/stop_times.txt" ] || fail "GTFS data not found in data/gtfs/. Run bin/download.sh first."
}

# Valhalla runs as its own container, started by bin/valhalla.sh (the Docker
# mode brings its own through Compose instead).
ensure_valhalla() {
    if ! "$ROOT/bin/valhalla.sh" status 2>/dev/null | grep -q "running"; then
        log "Valhalla is not running, starting it..."
        "$ROOT/bin/valhalla.sh" start
    else
        ok "Valhalla is already running."
    fi
}

# Rebuild when artifacts are missing or were built for another API origin,
# since the API origin is baked into the portal bundle.
ensure_artifacts() {
    local built_for=""
    [ -f "$ROOT/target/portal.api-url" ] && built_for="$(cat "$ROOT/target/portal.api-url")"
    if [ ! -x "$ROOT/target/release/glove" ] || [ ! -f "$ROOT/target/portal/index.html" ]; then
        log "Build artifacts missing — running bin/build.sh..."
        GLOVE_API_URL="$API_URL" "$ROOT/bin/build.sh"
    elif [ "$built_for" != "$API_URL" ]; then
        log "Portal built for '${built_for:-unknown}', expected $API_URL — rebuilding..."
        GLOVE_API_URL="$API_URL" "$ROOT/bin/build.sh"
    else
        ok "Using existing build artifacts (run bin/build.sh to rebuild)."
    fi
}

# `static` serves target/portal; `proxy` forwards to whatever listens on
# 127.0.0.1:3000 (Vite dev server or the portal container).
start_caddy() {
    local portal_mode="$1"
    log "Starting Caddy ($PORTAL_HOST, $API_HOST)..."
    GLOVE_PORTAL_MODE="$portal_mode" \
    GLOVE_PORTAL_HOST="$PORTAL_HOST" \
    GLOVE_API_HOST="$API_HOST" \
    GLOVE_PORTAL_ROOT="$ROOT/target/portal" \
    GLOVE_HTTP_PORT="$HTTP_PORT" \
    GLOVE_HTTPS_PORT="$HTTPS_PORT" \
        caddy run --config "$ROOT/deploy/Caddyfile" --adapter caddyfile &
    CADDY_PID=$!
    sleep 1
    kill -0 "$CADDY_PID" 2>/dev/null || fail "Caddy failed to start (see its log above)."
}

# `alive_check` tells a crashed backend apart from one still loading.
wait_for_backend() {
    local alive_check="$1"
    for _ in $(seq 1 180); do
        if curl -sf -o /dev/null "http://localhost:8080/api/status" 2>/dev/null; then
            ok "Backend ready."
            return
        fi
        $alive_check || fail "Backend failed to start."
        sleep 1
    done
    warn "Backend still loading after 180 s; Caddy answers 502 until it is up."
}

backend_process_alive() { kill -0 "$BACKEND_PID" 2>/dev/null; }

api_container_alive() {
    [ "$($DOCKER compose -f "$COMPOSE_FILE" ps --status running --services 2>/dev/null | grep -c '^api$')" -eq 1 ]
}

# Caddy signs the two names with its own local CA; browsers warn until its
# root certificate is trusted.
print_trust_hint() {
    local root_crt="${XDG_DATA_HOME:-$HOME/.local/share}/caddy/pki/authorities/local/root.crt"
    [ -f "$root_crt" ] || return
    log "Certificates come from Caddy's local CA. Trust it once:"
    log "    Linux (curl, Chrome/Firefox on Linux): caddy trust"
    if grep -qi microsoft /proc/version 2>/dev/null; then
        log "    Windows browser (from WSL, current user, no admin):"
        log "    certutil.exe -user -addstore Root \"\$(wslpath -w $root_crt)\""
    fi
}

print_banner() {
    echo ""
    ok "==========================="
    ok "  $1"
    ok "  Portal: $PORTAL_URL"
    ok "  API:    $API_URL"
    ok "==========================="
    echo ""
    print_trust_hint
    log "Press Ctrl+C to stop."
}

run_dev() {
    log "Starting in DEV mode (hot-reload)..."
    command -v cargo-watch >/dev/null || fail "cargo-watch not found. Install it: cargo install cargo-watch"
    ensure_valhalla

    log "Starting backend (cargo-watch)..."
    cargo watch -x run -w src -w config.yaml -q &
    BACKEND_PID=$!

    (cd "$ROOT/portal" && npm install --silent)

    # Loopback only: the dev server is reached through Caddy. Vite accepts the
    # portal host name through server.allowedHosts (vite.config.js). `exec`
    # the local binary, not `npx`, so FRONTEND_PID is Vite itself and cleanup
    # does not leave it running.
    log "Starting frontend (vite dev)..."
    (cd "$ROOT/portal" && GLOVE_PORTAL_HOST="$PORTAL_HOST" VITE_API_URL="$API_URL" \
        exec ./node_modules/.bin/vite --host 127.0.0.1) &
    FRONTEND_PID=$!

    start_caddy proxy
    print_banner "Glove DEV mode (HMR + hot-reload)"
}

run_prod() {
    log "Starting in PROD mode..."
    ensure_valhalla
    ensure_artifacts

    log "Starting backend..."
    "$ROOT/target/release/glove" &
    BACKEND_PID=$!

    # Caddy serves the portal build itself (no Node process in production).
    start_caddy static
    wait_for_backend backend_process_alive
    print_banner "Glove is running"
}

# `docker compose up` with the variables docker-compose.yml reads: the api
# container runs as the owner of data/ (it writes there), and GLOVE_API_KEY,
# when set, replaces the key of config.yaml. Passed through `env` because a
# `sudo docker` would otherwise strip them from the environment.
compose_up() {
    local vars=("GLOVE_UID=$(id -u)" "GLOVE_GID=$(id -g)")
    [ -n "${GLOVE_API_KEY+set}" ] && vars+=("GLOVE_API_KEY=$GLOVE_API_KEY")
    local sudo=""
    [ "$DOCKER" = "sudo docker" ] && sudo="sudo"
    $sudo env "${vars[@]}" docker compose -f "$COMPOSE_FILE" up -d --build
}

# The images are rebuilt on every start; Docker's layer cache makes that
# cheap when nothing changed. The portal container proxies /api to the api
# container itself, so the SPA is same-origin and needs no CORS setup;
# api.glove still exposes the API to other clients.
run_docker() {
    log "Starting in DOCKER mode (api, portal, valhalla images)..."
    log "Building and starting containers..."
    COMPOSE_STARTED=true
    compose_up

    start_caddy proxy
    wait_for_backend api_container_alive

    $DOCKER compose -f "$COMPOSE_FILE" logs -f --no-log-prefix api portal &
    LOGS_PID=$!
    print_banner "Glove is running (Docker)"
}

# Parse options
MODE=prod
case "${1:-}" in
    "")       ;;
    --dev)    MODE=dev ;;
    --docker) MODE=docker ;;
    *)        fail "Unknown option: $1 (usage: bin/start.sh [--dev | --docker])" ;;
esac

check_prerequisites
case "$MODE" in
    dev)    run_dev ;;
    docker) run_docker ;;
    prod)   run_prod ;;
esac

wait
