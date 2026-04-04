#!/usr/bin/env bash
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG="$ROOT/config.yaml"

GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${CYAN}[glove]${NC} $1"; }
ok()   { echo -e "${GREEN}[glove]${NC} $1"; }
fail() { echo -e "${RED}[glove]${NC} $1"; exit 1; }

confirm() {
    read -rp $'\033[0;36m[glove]\033[0m '"$1"' (y/N) ' answer
    [[ "$answer" =~ ^[yYoO]$ ]]
}

usage() {
    echo "Usage: $0 <all|osm|gtfs|ban|sirene>"
    echo ""
    echo "  all    Download OSM, GTFS, BAN and SIRENE data"
    echo "  osm    Download OSM data only"
    echo "  gtfs   Download GTFS data only"
    echo "  ban    Download BAN address data only"
    echo "  sirene Download SIRENE company database only"
    exit 1
}

# Read a value from config.yaml (no defaults — exits if missing)
yaml_val() {
    local val
    val=$(grep "^$1:" "$CONFIG" 2>/dev/null | sed 's/^[^:]*:[[:space:]]*//' | tr -d '"')
    if [ -z "$val" ]; then
        fail "Missing key '$1' in $CONFIG"
    fi
    echo "$val"
}

# --- Prerequisites ---
[ -n "$1" ] || usage
[ -f "$CONFIG" ] || fail "Config file not found: $CONFIG"
command -v wget >/dev/null || fail "wget not found. Install it first."
command -v unzip >/dev/null || fail "unzip not found. Install it first."
command -v gunzip >/dev/null || fail "gunzip not found. Install it first."

# --- Read config ---
OSM_DIR="$ROOT/$(yaml_val osm_dir)"
OSM_URL="$(yaml_val osm_url)"
OSM_FILE="$OSM_DIR/$(basename "$OSM_URL")"

GTFS_DIR="$ROOT/$(yaml_val data_dir)"
GTFS_URL="$(yaml_val gtfs_url)"
GTFS_ZIP="$GTFS_DIR/gtfs-idfm.zip"

BAN_DIR="$ROOT/$(yaml_val ban_dir)"
BAN_BASE_URL="$(yaml_val ban_url)"
DEPARTMENTS=$(grep "^departments:" "$CONFIG" | sed 's/^[^[]*\[//;s/\].*//;s/,/ /g;s/  */ /g;s/^ //;s/ $//')

SIRENE_DIR="$ROOT/$(yaml_val sirene_dir)"
SIRENE_URL="$(yaml_val sirene_url)"
SIRENE_FILE="$SIRENE_DIR/base-sirene.csv"

# --- Functions ---
download_osm() {
    mkdir -p "$OSM_DIR"
    if [ -f "$OSM_FILE" ]; then
        log "OSM file already exists: $OSM_FILE"
        if confirm "Replace it?"; then
            log "Downloading OSM data..."
            wget -O "$OSM_FILE" "$OSM_URL"
            ok "OSM data saved to $OSM_FILE"
        else
            log "Skipping OSM download."
        fi
    else
        log "Downloading OSM data..."
        wget -O "$OSM_FILE" "$OSM_URL"
        ok "OSM data saved to $OSM_FILE"
    fi
}

download_gtfs() {
    mkdir -p "$GTFS_DIR"
    if [ -f "$GTFS_DIR/stop_times.txt" ]; then
        log "GTFS data already exists in $GTFS_DIR"
        if confirm "Replace it?"; then
            log "Downloading GTFS data..."
            wget -O "$GTFS_ZIP" "$GTFS_URL"
            log "Extracting GTFS data..."
            unzip -o "$GTFS_ZIP" -d "$GTFS_DIR"
            rm "$GTFS_ZIP"
            ok "GTFS data extracted to $GTFS_DIR"
        else
            log "Skipping GTFS download."
        fi
    else
        log "Downloading GTFS data..."
        wget -O "$GTFS_ZIP" "$GTFS_URL"
        log "Extracting GTFS data..."
        unzip -o "$GTFS_ZIP" -d "$GTFS_DIR"
        rm "$GTFS_ZIP"
        ok "GTFS data extracted to $GTFS_DIR"
    fi
}

download_ban() {
    mkdir -p "$BAN_DIR"
    if [ -z "$DEPARTMENTS" ]; then
        fail "No departments configured in $CONFIG"
    fi
    for dept in $DEPARTMENTS; do
        local csv_file="$BAN_DIR/adresses-${dept}.csv"
        local gz_file="${csv_file}.gz"
        local url="${BAN_BASE_URL}/adresses-${dept}.csv.gz"
        if [ -f "$csv_file" ]; then
            log "BAN file already exists: $csv_file"
            if confirm "Replace it?"; then
                log "Downloading BAN data for department ${dept}..."
                wget -O "$gz_file" "$url"
                gunzip -f "$gz_file"
                ok "BAN data extracted to $csv_file"
            else
                log "Skipping department ${dept}."
            fi
        else
            log "Downloading BAN data for department ${dept}..."
            wget -O "$gz_file" "$url"
            gunzip -f "$gz_file"
            ok "BAN data extracted to $csv_file"
        fi
    done
}

download_sirene() {
    mkdir -p "$SIRENE_DIR"
    if [ -f "$SIRENE_FILE" ]; then
        log "SIRENE file already exists: $SIRENE_FILE"
        if confirm "Replace it?"; then
            log "Downloading SIRENE database..."
            wget -O "$SIRENE_FILE" "$SIRENE_URL"
            ok "SIRENE data saved to $SIRENE_FILE"
        else
            log "Skipping SIRENE download."
        fi
    else
        log "Downloading SIRENE database..."
        wget -O "$SIRENE_FILE" "$SIRENE_URL"
        ok "SIRENE data saved to $SIRENE_FILE"
    fi
}

# --- Main ---
case "$1" in
    all)
        download_osm
        download_gtfs
        download_ban
        download_sirene
        ;;
    osm)
        download_osm
        ;;
    gtfs)
        download_gtfs
        ;;
    ban)
        download_ban
        ;;
    sirene)
        download_sirene
        ;;
    *)
        usage
        ;;
esac
