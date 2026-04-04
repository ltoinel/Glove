# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Glove is a public transit journey planner. Rust backend (Actix-web) with the RAPTOR algorithm, React frontend (MUI + Leaflet), GTFS data, and optional Valhalla for walking routes.

## Build & Run Commands

### Backend (Rust)
```bash
cargo build --release        # Build release binary
cargo build                  # Build debug
cargo test                   # Run all tests
cargo clippy -- -D warnings  # Lint (CI enforced)
cargo fmt --check            # Format check (CI enforced)
cargo fmt                    # Auto-format
```

### Frontend (React)
```bash
cd portal
npm install                  # Install dependencies
npm run dev                  # Vite dev server with HMR
npm run build                # Production build
npx eslint src/              # Lint (CI enforced)
```

### Full Stack
```bash
bin/download.sh              # Download GTFS + OSM data (reads config.yaml)
bin/valhalla.sh              # Start Valhalla Docker container (port 8002)
bin/start.sh                 # Production: builds and starts everything
bin/start.sh --dev           # Dev: cargo-watch + Vite HMR
```

## Architecture

### RAPTOR Algorithm (`src/raptor.rs`)
Core of the application. Round-based public transit routing with:
- **Pre-processing** (10-30s on startup): builds stop index, interns service IDs, groups trips into patterns (identical stop sequences), builds transfer graph
- **Query**: runs rounds (each = one additional vehicle trip), with calendar-aware service filtering and pattern exclusion for route diversity
- **Reconstruction**: traces labels backward, sanitizes sections, returns Pareto-optimal journeys
- Fuzzy stop search with French diacritics normalization (exact > prefix > word-prefix > substring ranking)

### Data Flow
1. `src/main.rs` loads config (`src/config.rs`) and GTFS CSVs (`src/gtfs.rs`)
2. Builds `RaptorData` index, wraps in `ArcSwap` for lock-free hot-reload
3. Actix-web serves API + static frontend files

### API Endpoints
- `GET /api/journeys/public_transport` — RAPTOR journey planning (Navitia-compatible query params)
- `GET /api/journeys/walk` — Walking directions via Valhalla
- `GET /api/places` — Stop autocomplete (fuzzy search)
- `GET /api/status` — GTFS stats and last load timestamp
- `POST /api/reload` — Hot-reload GTFS data without downtime (atomic swap via ArcSwap)

### Frontend (`portal/`)
Single-page app: left sidebar (search/results/settings) + full-height Leaflet map. All in `App.jsx` (~1065 lines). Dark theme with CARTO basemap. i18n for FR/EN in `i18n.jsx`. Queries public_transport and walk endpoints in parallel.

### Key Design Decisions
- **All in-memory**: no database, GTFS loaded from CSV at startup
- **Lock-free hot-reload**: `ArcSwap` swaps entire RAPTOR index atomically
- **Pattern grouping**: trips with identical stop sequences share a pattern (memory + speed)
- **Iterative diverse search**: runs RAPTOR multiple times with pattern exclusion for varied alternatives
- **Navitia API compatibility**: mirrors Navitia query parameters for drop-in replacement

## Configuration

`config.yaml` at repo root. Key settings: `data_dir` (GTFS path), `valhalla_host`/`valhalla_port` (walking router), `max_journeys`, `max_transfers`, `default_transfer_time` (seconds), `max_duration` (seconds), `workers` (0 = auto).

## CI

GitHub Actions (`.github/workflows/ci.yml`): Rust format + clippy + build + test, then Node ESLint + Vite build.
