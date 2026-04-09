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
- `GET /api/journeys/bike` — Cycling directions via Valhalla (city, ebike, road profiles)
- `GET /api/journeys/car` — Driving directions via Valhalla
- `GET /api/places` — Stop autocomplete (fuzzy search)
- `GET /api/status` — GTFS stats and last load timestamp
- `GET /api/gtfs/validate` — GTFS data quality validation (19 checks)
- `POST /api/gtfs/reload` — Hot-reload GTFS data without downtime (atomic swap via ArcSwap)
- `GET /api/tiles/{z}/{x}/{y}.png` — Map tile proxy with local disk cache

### Frontend (`portal/`)
Single-page app: vertical nav rail (56px) + sidebar (450px) + Leaflet map. Dark theme with cached CARTO tiles. i18n for FR/EN in `i18n.jsx`. Queries all endpoints in parallel (PT, walk, bike, car). Views: search (default), GTFS validation, dataset, swagger, metrics. Pure utility functions in `utils.js`, tested with vitest.

### Key Design Decisions
- **All in-memory**: no database, GTFS loaded from CSV at startup
- **Lock-free hot-reload**: `ArcSwap` swaps entire RAPTOR index atomically
- **Pattern grouping**: trips with identical stop sequences share a pattern (memory + speed)
- **Iterative diverse search**: runs RAPTOR multiple times with pattern exclusion for varied alternatives
- **Navitia API compatibility**: mirrors Navitia query parameters for drop-in replacement
- **Tile caching proxy**: map tiles fetched from upstream once, cached to `data/tiles/` on disk
- **Indoor-aware transfers**: Valhalla pedestrian routing with zero step/elevator penalties for intra-station walks
- **After-midnight routing**: queries before 4h use previous day's GTFS services with +86400s offset
- **Station-aware stop resolution**: stop IDs resolve to the stop itself + child stops sharing the same parent_station

## Configuration

`config.yaml` at repo root. Key settings: `data_dir` (GTFS path), `valhalla_host`/`valhalla_port` (walking router), `max_journeys`, `max_transfers`, `default_transfer_time` (seconds), `max_duration` (seconds), `workers` (0 = auto), `map.tile_url` (upstream tile server URL template with `{s}`, `{z}`, `{x}`, `{y}`, `{r}` placeholders).

## Clean Code Principles

This codebase follows Clean Code practices (Robert C. Martin). All contributions must respect:

### Naming
- **Descriptive names**: functions, variables, and types must be self-explanatory (`build_stop_index`, not `bsi`)
- **Consistent vocabulary**: use the same term for the same concept across the codebase (e.g. `stop_idx` everywhere, not `stop_index` in one place and `idx` in another)

### Functions
- **Small and focused**: each function does one thing. Target ~40 lines max
- **Few parameters**: prefer 3 or fewer. Bundle related params into structs when needed
- **One level of abstraction**: a function should not mix high-level orchestration with low-level details

### DRY (Don't Repeat Yourself)
- **Shared utilities** in `src/util.rs`: `parse_coord`, `parse_from_to`, `dir_fingerprint`
- **Shared Valhalla types** in `src/api/journeys/valhalla.rs`: `Location`, `RouteRequest`, `RouteResponse`, etc.
- **No copy-pasted blocks**: if the same pattern appears 3+ times, extract a function

### Error Handling
- **No `unwrap()` in production code** — use `?`, `unwrap_or_else`, or explicit error handling
- **No silent swallowing** — log at `warn!` or `debug!` level when ignoring errors
- **Propagate errors** with `Result<T, E>` instead of returning sentinel values (0, empty vec)

### Single Responsibility
- Each module has a clear scope (see Architecture section)
- `src/raptor.rs` build logic is split into sub-functions: `build_stop_index`, `intern_services`, `build_patterns`, `build_transfers`, `build_search_index`
- API handlers delegate to helper functions for enrichment and tagging

### Constants over Magic Numbers
- Named constants for thresholds and limits (`INFINITY`, `MAX_ROUNDS`, `ELEVATION_SAMPLE_LIMIT`)
- GTFS route types (0=tram, 1=metro, etc.) are documented inline where used

### Comments
- **Explain why, not what** — code should be self-documenting for the "what"
- **Doc comments** (`///`) on all public types and functions
- **Algorithm comments** for non-obvious logic (Dijkstra, RAPTOR rounds, polyline decoding)

### React / Frontend
- **All user-facing strings** must use `t()` from `useI18n()` — no hardcoded text
- **`useCallback`** on event handlers passed to children (`search`, `swap`, `handleFromChange`, `refreshStatus`)
- **Safe localStorage** — always wrap `JSON.parse()` in try-catch
- **Accessibility** — all `IconButton` must have `aria-label`; prefer semantic `<button>` over `<div onClick>`
- **Error handling** — fetch `.catch()` must log with `console.warn`, never silently swallow
- **Next step**: split `App.jsx` into component files (`components/`) when test coverage allows safe refactoring

## CI

GitHub Actions (`.github/workflows/ci.yml`): Rust format + clippy + build + test, then Node ESLint + Vite build.
