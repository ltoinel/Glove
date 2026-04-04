#!/usr/bin/env bash
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OSM_DIR="$ROOT/data/osm"
VALHALLA_DIR="$ROOT/data/valhalla"
CONTAINER_NAME="glove-valhalla"

GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${CYAN}[glove]${NC} $1"; }
ok()   { echo -e "${GREEN}[glove]${NC} $1"; }
fail() { echo -e "${RED}[glove]${NC} $1"; exit 1; }

command -v docker >/dev/null || fail "docker not found. Install Docker first."
ls "$OSM_DIR"/*.pbf >/dev/null 2>&1 || fail "No .pbf files found in $OSM_DIR. Run ./download.sh first."

# Use sudo if user is not in docker group
DOCKER="docker"
if ! docker info >/dev/null 2>&1; then
    log "Docker requires elevated permissions, using sudo..."
    DOCKER="sudo docker"
fi

# Stop existing container if running
if $DOCKER ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    log "Stopping existing $CONTAINER_NAME container..."
    $DOCKER rm -f "$CONTAINER_NAME" >/dev/null
fi

log "Pulling latest Valhalla image..."
$DOCKER pull ghcr.io/gis-ops/docker-valhalla/valhalla:latest

log "Starting Valhalla on port 8002..."
mkdir -p "$VALHALLA_DIR"

$DOCKER run -d \
    --name "$CONTAINER_NAME" \
    -p 8002:8002 \
    -v "$OSM_DIR":/custom_files \
    -v "$VALHALLA_DIR":/custom_files/valhalla_tiles \
    -e use_tiles_ignore_pbf=False \
    -e force_rebuild=False \
    -e build_elevation=True \
    -e build_admins=True \
    -e build_time_zones=True \
    -e server_threads=2 \
    ghcr.io/gis-ops/docker-valhalla/valhalla:latest

ok "Valhalla container started: $CONTAINER_NAME"
log "Building tiles from OSM data (this may take a while)..."
log "Follow progress with: $DOCKER logs -f $CONTAINER_NAME"
log "API will be available at http://localhost:8002 once ready."
