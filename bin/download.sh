#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# download.sh — Download external data files required by Glove.
#
# Reads URLs and directory paths from config.yaml (nested YAML format),
# then fetches GTFS, OSM and BAN data into the configured data directory.
#
# Usage:
#   bin/download.sh all    # Download everything
#   bin/download.sh gtfs   # GTFS transit schedules only
#   bin/download.sh osm    # OpenStreetMap extract only
#   bin/download.sh ban    # BAN address database only
#
# Each download is idempotent: if the target file already exists, the user
# is prompted before overwriting.
# ---------------------------------------------------------------------------
set -e

# Resolve the project root (parent of bin/)
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG="$ROOT/config.yaml"

# --- Terminal colors ---
GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

# --- Logging helpers ---
log()  { echo -e "${CYAN}[glove]${NC} $1"; }
ok()   { echo -e "${GREEN}[glove]${NC} $1"; }
fail() { echo -e "${RED}[glove]${NC} $1"; exit 1; }

# Prompt the user for confirmation (accepts y/Y/o/O).
confirm() {
    read -rp $'\033[0;36m[glove]\033[0m '"$1"' (y/N) ' answer
    [[ "$answer" =~ ^[yYoO]$ ]]
}

# Print usage and exit.
usage() {
    echo "Usage: $0 <all|osm|gtfs|ban>"
    echo ""
    echo "  all    Download OSM, GTFS and BAN data"
    echo "  osm    Download OSM data only"
    echo "  gtfs   Download GTFS data only"
    echo "  ban    Download BAN address data only"
    exit 1
}

# ---------------------------------------------------------------------------
# YAML reader — supports nested keys via dot notation.
#
# Examples:
#   yaml_val "data.dir"       → reads "dir:" under the "data:" section
#   yaml_val "server.port"    → reads "port:" under the "server:" section
# ---------------------------------------------------------------------------
yaml_val() {
    local val
    if [[ "$1" == *.* ]]; then
        # Nested key: extract the section name and the key within it,
        # then use sed to find the indented key under the section header.
        local section="${1%%.*}"
        local key="${1#*.}"
        val=$(sed -n "/^${section}:/,/^[^ ]/{ s/^  *${key}:[[:space:]]*//p; }" "$CONFIG" | head -1 | tr -d '"')
    else
        # Top-level key: simple grep.
        val=$(grep "^$1:" "$CONFIG" 2>/dev/null | sed 's/^[^:]*:[[:space:]]*//' | tr -d '"')
    fi
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

# --- Read configuration values ---
# All data lives under a single root directory; sub-directories are implicit.
DATA_DIR="$ROOT/$(yaml_val data.dir)"

OSM_DIR="$DATA_DIR/osm"
OSM_URL="$(yaml_val data.osm_url)"
OSM_FILE="$OSM_DIR/$(basename "$OSM_URL")"

GTFS_DIR="$DATA_DIR/gtfs"
GTFS_URL="$(yaml_val data.gtfs_url)"
GTFS_ZIP="$GTFS_DIR/gtfs-idfm.zip"

BAN_DIR="$DATA_DIR/ban"
BAN_BASE_URL="$(yaml_val data.ban_url)"
# Parse the departments array from the nested "data:" section.
DEPARTMENTS=$(sed -n '/^data:/,/^[^ ]/{s/^  *departments:[[:space:]]*\[//p;}' "$CONFIG" \
    | sed 's/\].*//;s/,/ /g;s/  */ /g;s/^ //;s/ $//')

# ---------------------------------------------------------------------------
# Download functions
# ---------------------------------------------------------------------------

# Download the OpenStreetMap PBF extract (used by Valhalla for routing tiles).
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

# Download and extract the GTFS archive (transit schedules).
# The ZIP is removed after extraction; presence of stop_times.txt is used
# as a sentinel to detect an existing dataset.
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

# Download BAN (Base Adresse Nationale) CSV files, one per department.
# Files are distributed as gzipped CSVs; each is decompressed in place.
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

# ---------------------------------------------------------------------------
# Main — dispatch on the first argument
# ---------------------------------------------------------------------------
case "$1" in
    all)
        download_osm
        download_gtfs
        download_ban
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
    *)
        usage
        ;;
esac
