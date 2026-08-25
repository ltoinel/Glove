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
bin/download.sh              # Download GTFS + OSM + BAN + traffic data (reads config.yaml)
bin/valhalla.sh              # Start Valhalla Docker container (port 8002)
bin/build.sh                 # Build release artifacts: backend binary + portal SPA
bin/start.sh                 # Production: run only (auto-runs build.sh if artifacts missing)
bin/start.sh --dev           # Dev: cargo-watch + Vite HMR
```

## Architecture

### Module Layout (`src/`)
Domain modules hold the business logic; `api/` is the only HTTP layer. Dependencies
flow one way — `api/` → domains → `shared/` — and no domain depends on another.

```
src/
├── main.rs        bootstrap: config load, index build, server wiring, OpenAPI
├── shared/        cross-cutting: config.rs, text.rs, util.rs
├── transit/       DOMAIN public transport: gtfs.rs, raptor.rs, realtime/,
│                                            disruptions/
├── geocoding/     DOMAIN addresses: ban.rs
├── traffic/       DOMAIN road traffic: sytadin.rs
└── api/           HTTP layer: journeys/, places.rs, gtfs.rs, traffic.rs,
                   realtime.rs, disruptions.rs, lines.rs, status.rs,
                   metrics.rs, tiles.rs
```

`realtime/` and `disruptions/` live inside `transit/` rather than beside it:
both name stops and lines of the loaded GTFS, neither means anything without
it, and `raptor` reads both overlays at query time. `text.rs`
sits in `shared/` because both `transit::raptor` (stop search) and
`geocoding::ban` (address search) normalize with it.

`main.rs` aliases the domain entry points (`use transit::{gtfs, raptor, realtime};`)
so the bootstrap reads by concept rather than by module path.

### RAPTOR Algorithm (`src/transit/raptor.rs`)
Core of the application. Round-based public transit routing with:
- **Pre-processing** (10-30s on startup): builds stop index, interns service IDs, groups trips into patterns (identical stop sequences), builds transfer graph
- **Query**: runs rounds (each = one additional vehicle trip), with calendar-aware service filtering and pattern exclusion for route diversity
- **Reconstruction**: traces labels backward, sanitizes sections, returns Pareto-optimal journeys
- Fuzzy stop search with French diacritics normalization (exact > prefix > word-prefix > substring ranking)

### Data Flow
1. `src/main.rs` loads config (`src/shared/config.rs`) and GTFS CSVs (`src/transit/gtfs.rs`)
2. Builds `RaptorData` index, wraps in `ArcSwap` for lock-free hot-reload
3. Actix-web serves the REST API only (port 8080). The React portal runs as a **separate process** — Vite dev server in dev, `vite preview` in prod (port 3000) — and proxies `/api` to the backend (`portal/vite.config.js`)

### API Endpoints
- `GET /api/journeys/public_transport` — RAPTOR journey planning
- `GET /api/journeys/walk` — Walking directions via Valhalla
- `GET /api/journeys/bike` — Cycling directions via Valhalla (city, ebike, road profiles)
- `GET /api/journeys/car` — Driving directions via Valhalla
- `GET /api/places` — Stop autocomplete (fuzzy search)
- `GET /api/status` — engine health (dependencies) and map defaults only (no GTFS data)
- `GET /api/gtfs/status` — GTFS data statistics and last load timestamp
- `GET /api/gtfs/validate` — GTFS data quality validation (19 checks)
- `POST /api/gtfs/reload` — Hot-reload GTFS data without downtime (atomic swap via ArcSwap)
- `GET /api/metrics` — Prometheus-format metrics (HTTP counters, CPU, memory)
- `GET /api/realtime/status` — Real-time transit feed health + schedule-matching counters
- `GET /api/lines` — Line catalogue for the back-office pickers (`?q=` name filter)
- `GET|POST /api/disruptions`, `GET|PUT|DELETE /api/disruptions/{id}` — Operator-authored disruption CRUD (writes need `X-Api-Key`)
- `GET /api/disruptions/active` — Blocking disruptions in force now, resolved to map coordinates (closed stops + cut segments)
- `GET /api/traffic/geometry` — Road-network polylines for the traffic overlay (static, cacheable 24 h)
- `GET /api/traffic/states` — Live segment states + events, no coordinates (joined client-side on segment id)
- `GET /api/tiles/{z}/{x}/{y}.png` — Map tile proxy with local disk cache
- `GET /api-docs/openapi.json` — Auto-generated OpenAPI specification

### Frontend (`portal/`)
Single-page app: vertical nav rail (56px) + sidebar (450px) + Leaflet map. Dark theme with cached CARTO tiles. i18n for FR/EN in `i18n.jsx`. Queries all endpoints in parallel (PT, walk, bike, car). Views: search (default), GTFS validation, disruptions back office, dataset, swagger, metrics. The disruption admin screen is `components/DisruptionsPanel.jsx` (lazy-loaded) and keeps the API key in `localStorage`. Two map overlays sit top-right, each polled only while displayed: road traffic and current blockages (`DisruptionLayer`). Pure utility functions in `utils.js`, tested with vitest.

### Real-time transit (`src/transit/realtime/`)
Delays and cancellations applied at query time, never by rebuilding the index.
- `model.rs` — connector-agnostic pivot model (`TripUpdate`, `StopTimeUpdate`). GTFS-RT vocabulary, because SIRI maps onto it cleanly and not the reverse
- `source.rs` — `RealtimeSource` trait (object-safe, boxed future). Adding a format = one file
- `protobuf.rs` — minimal wire-format reader (~150 lines, zero deps). `prost-build` would require `protoc`, absent from CI
- `gtfs_rt.rs` — GTFS-Realtime connector; `VehiclePosition`/`Alert` entities are skipped as unknown fields
- `index.rs` — resolves feeds against the schedule into a `RealtimeIndex` overlay keyed by `(pattern_idx, trip_idx)`
- `service.rs` — one polling task per feed, `ArcSwapOption` publication, per-feed health

**Phase 1 scope**: delays + cancellations on scheduled trips. `ADDED` trips are counted as unsupported (injecting unscheduled stop sequences into patterns is separate work).

### Disruptions (`src/transit/disruptions/`)
Works, incidents and closures entered by hand in the back office, applied at
query time like the real-time overlay.
- `model.rs` — what an operator declares: `Scope` (`Stop` / `Line` / `LineSection`), `Severity` (`Blocking` / `Info`), `Period` (start + optional end, absent = ongoing)
- `store.rs` — the catalog: one JSON document, `ArcSwap` for lock-free reads, a mutex for the rare writes, temp-file + rename for atomic persistence
- `overlay.rs` — resolves identifiers into stop/pattern indices for the disruptions in force at a given instant, maps a reconstructed journey to the disruptions touching it, and (`blocked_geometry`) turns what is removed into stops + deduplicated edges for the map overlay

**Routing effects**: a blocked *stop* is neutralized entirely (no boarding, no
alighting, no transfer) while vehicles still run through it; a blocked *line*
is unioned into the router's `excluded_patterns`; a blocked *section* cuts
rides between its endpoints, in both directions, leaving the rest of the line
usable. `Info` severity annotates without removing anything.

### Key Design Decisions
- **All in-memory**: no database, GTFS loaded from CSV at startup
- **Lock-free hot-reload**: `ArcSwap` swaps entire RAPTOR index atomically
- **Pattern grouping**: trips with identical stop sequences share a pattern (memory + speed)
- **Iterative diverse search**: runs RAPTOR multiple times with pattern exclusion for varied alternatives. Optional `routing.diverse_lines` additionally excludes the whole head line between iterations so each alternative departs on a different line. Optional `routing.prefer_rail` runs a first tier with buses forbidden so rail journeys are found first, buses filling only remaining slots (`collect_alternatives` tiers in `run_iterative_search`)
- **Server-controlled routing settings**: number of journeys (`max_journeys`), transfers (`max_transfers`), `diverse_lines`, `prefer_rail` and `maneuvers` are config-only (`config.yaml`), intentionally NOT overridable via request parameters
- **Tile caching proxy**: map tiles fetched from upstream once, cached to `data/tiles/` on disk
- **Indoor-aware transfers**: Valhalla pedestrian routing with zero step/elevator penalties for intra-station walks
- **Traffic overlay, split by lifetime**: the Sytadin MIF/MID geometry is parsed once at startup (Lambert II étendu → WGS84, `src/traffic/sytadin.rs`) and served as an immutable ~860 kB body cached 24 h by the browser; only the states (~175 kB, no coordinates) are polled and re-published via `ArcSwapOption` (`src/api/traffic.rs`). Both bodies are serialized once, never per request. Disabled by default, degrades to `enabled: false` when the geometry is missing
- **Real-time as an overlay, not a rebuild**: pre-processing takes 10-30 s and feeds refresh every 30 s, so predictions are resolved into a `RealtimeIndex` and swapped atomically. The router reads schedule + overlay; `RaptorData` is never touched
- **Calls matched by `stop_id`, not `stop_sequence`**: `build_patterns` sorts calls by `stop_sequence` but discards the values, so a position cannot be recovered from it. A forward-only cursor keeps loop routes in order
- **Real-time widens the trip-scan window**: trips are sorted by *scheduled* departure and scanned from a fixed look-back before the binary-search pivot. Offsets break that order, so the window widens by the pattern's largest published offset (`PatternDeltas::max_abs_delta`) — otherwise a delayed vehicle is invisible
- **Trips that skip a call are set aside like cancellations**: a dropped call no longer matches the pattern's stop sequence, and re-splitting patterns per refresh is exactly the pre-processing the overlay exists to avoid. Pessimistic for that trip's other passengers, safe for everyone
- **Feed identifiers are the integration risk**: a feed can answer 200 with a valid body and match nothing when its namespace differs from the GTFS. `MatchStats` on `/api/realtime/status` makes that visible
- **After-midnight routing**: queries before 4h use previous day's GTFS services with +86400s offset
- **Station-aware stop resolution**: stop IDs resolve to the stop itself + child stops sharing the same parent_station
- **Disruptions are authored, so they are persisted**: everything else is rebuilt from a source file on startup; a disruption cannot be. That is a JSON document rewritten whole (temp file + rename), not a database — operators author tens to hundreds of them
- **A blocked journey is returned, not hidden**: "the fastest route is closed, here is why" beats silently offering a slower alternative with no explanation. When a blocking disruption is in force, a second *undisrupted* RAPTOR pass recovers the journey the traveller would have taken; it comes back with `status: "blocked"` and the disruptions explaining it. It has to be a second pass: once a stop leaves the graph, the journey through it leaves no trace to report
- **A blocked journey never wins a quality tag**: `tag_journeys` computes its minima over usable journeys only, so "fastest" cannot land on something nobody can take
- **Disruption periods are wall-clock**: resolved against the query datetime *before* the early-morning day shift, so a 01:00 query is not matched against the previous day at 25:00
- **Closing a stop closes its station**: `expand_station` widens a stop id upward to its parent and downward to every child, so an operator closes "Châtelet" without listing its platforms
- **The blockage overlay ships edges, not stop sequences**: a line closure blocks every ride of every pattern of that route, and those patterns overlap heavily. `blocked_geometry` collapses them to a direction-normalized, deduplicated edge set, so the payload is bounded by the network's topology rather than its pattern count
- **Blockage segments are schematic**: they join consecutive stops in a straight line. GTFS `shapes.txt` is not loaded, so the overlay shows *what* is cut, not the exact track alignment

## Configuration

`config.yaml` at repo root. Real-time feeds live under `realtime.feeds` (`type`, `url`, `refresh_secs`, `timeout_secs`, `headers`); `FeedConfig`'s `Debug` impl redacts header values and URL query strings so `info!(?config)` cannot log API keys. Key settings: `data_dir` (GTFS path), `valhalla_host`/`valhalla_port` (walking router), `max_journeys`, `max_transfers`, `default_transfer_time` (seconds), `max_duration` (seconds), `workers` (0 = auto), `map.tile_url` (upstream tile server URL template with `{s}`, `{z}`, `{x}`, `{y}`, `{r}` placeholders), `traffic.enabled`/`traffic.base_url`/`traffic.refresh_secs` (Sytadin road traffic overlay). `server.api_key` guards both `POST /api/gtfs/reload` and every disruption write; the catalog lives at `{data.dir}/disruptions/disruptions.json`.

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
- **Shared utilities** in `src/shared/util.rs`: `parse_coord`, `parse_from_to`, `dir_fingerprint`
- **Shared Valhalla types** in `src/api/journeys/valhalla.rs`: `Location`, `RouteRequest`, `RouteResponse`, etc.
- **No copy-pasted blocks**: if the same pattern appears 3+ times, extract a function

### Error Handling
- **No `unwrap()` in production code** — use `?`, `unwrap_or_else`, or explicit error handling
- **No silent swallowing** — log at `warn!` or `debug!` level when ignoring errors
- **Propagate errors** with `Result<T, E>` instead of returning sentinel values (0, empty vec)

### Single Responsibility
- Each module has a clear scope (see Architecture section)
- `src/transit/raptor.rs` build logic is split into sub-functions: `build_stop_index`, `intern_services`, `build_patterns`, `build_transfers`, `build_search_index`
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
