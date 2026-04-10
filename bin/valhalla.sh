#!/usr/bin/env bash
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG="$ROOT/config.yaml"
OSM_DIR="$ROOT/data/osm"
VALHALLA_DIR="$ROOT/data/valhalla"
CONTAINER_NAME="glove-valhalla"

# Read Valhalla port from config.yaml (default: 8002)
VALHALLA_PORT=8002
if [ -f "$CONFIG" ]; then
    port=$(grep -A5 '^valhalla:' "$CONFIG" | grep 'port:' | head -1 | awk '{print $2}')
    [ -n "$port" ] && VALHALLA_PORT="$port"
fi

GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m'

log()  { echo -e "${CYAN}[glove]${NC} $1"; }
ok()   { echo -e "${GREEN}[glove]${NC} $1"; }
warn() { echo -e "${YELLOW}[glove]${NC} $1"; }
fail() { echo -e "${RED}[glove]${NC} $1"; exit 1; }

command -v docker >/dev/null || fail "docker not found. Install Docker first."

# Use sudo if user is not in docker group
DOCKER="docker"
if ! docker info >/dev/null 2>&1; then
    log "Docker requires elevated permissions, using sudo..."
    DOCKER="sudo docker"
fi

usage() {
    echo "Usage: $(basename "$0") {start|stop|status}"
    echo ""
    echo "  start   Start the Valhalla routing container"
    echo "  stop    Stop the Valhalla routing container"
    echo "  status  Show the container status"
    exit 1
}

do_start() {
    ls "$OSM_DIR"/*.pbf >/dev/null 2>&1 || fail "No .pbf files found in $OSM_DIR. Run ./download.sh first."

    # Stop existing container if running
    if $DOCKER ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log "Stopping existing $CONTAINER_NAME container..."
        $DOCKER rm -f "$CONTAINER_NAME" >/dev/null
    fi

    log "Pulling latest Valhalla image..."
    $DOCKER pull ghcr.io/gis-ops/docker-valhalla/valhalla:latest

    log "Starting Valhalla on port $VALHALLA_PORT..."
    mkdir -p "$VALHALLA_DIR"

    $DOCKER run -d \
        --name "$CONTAINER_NAME" \
        -p "$VALHALLA_PORT":8002 \
        -v "$OSM_DIR":/custom_files \
        -v "$VALHALLA_DIR":/custom_files/valhalla_tiles \
        -e use_tiles_ignore_pbf=False \
        -e force_rebuild=False \
        -e build_elevation=True \
        -e build_admins=True \
        -e build_time_zones=True \
        -e build_transit=False \
        -e server_threads=2 \
        -e include_platforms=True \
        ghcr.io/gis-ops/docker-valhalla/valhalla:latest

    ok "Valhalla container started: $CONTAINER_NAME"
    log "Building tiles from OSM data (this may take a while)..."
    log "Follow progress with: $DOCKER logs -f $CONTAINER_NAME"
    log "API will be available at http://localhost:$VALHALLA_PORT once ready."
}

do_stop() {
    if $DOCKER ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log "Stopping $CONTAINER_NAME..."
        $DOCKER rm -f "$CONTAINER_NAME" >/dev/null
        ok "Valhalla container stopped."
    else
        warn "Valhalla container is not running."
    fi
}

do_status() {
    if $DOCKER ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        ok "Valhalla is running."
        $DOCKER ps --filter "name=^${CONTAINER_NAME}$" --format "table {{.Status}}\t{{.Ports}}"
    elif $DOCKER ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        warn "Valhalla container exists but is stopped."
        $DOCKER ps -a --filter "name=^${CONTAINER_NAME}$" --format "table {{.Status}}"
    else
        warn "Valhalla container does not exist."
    fi
}

case "${1:-}" in
    start)  do_start ;;
    stop)   do_stop ;;
    status) do_status ;;
    *)      usage ;;
esac
