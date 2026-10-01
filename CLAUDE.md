# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Glove is a public transit journey planner: Rust backend (Actix-web) running the RAPTOR algorithm on GTFS data, React portal (MUI + Leaflet), Valhalla for walk/bike/car routing and first/last-mile walks. User and contributor documentation lives in the mdBook under `book/src/` — update it when behavior or settings change.

## Build & Run Commands

```bash
# Backend
cargo build [--release]
cargo test
cargo clippy -- -D warnings  # CI enforced
cargo fmt --check            # CI enforced

# Frontend
cd portal && npm install
npm run dev                  # Vite dev server with HMR
npm run build
npm run lint                 # eslint . (config files included) — CI enforced
npm test                     # vitest run

# Full stack
bin/download.sh              # GTFS + OSM + BAN + traffic data (reads config.yaml)
bin/valhalla.sh start        # Valhalla container glove-valhalla on :8002 (stop|status)
bin/build.sh                 # Release backend binary + portal SPA
bin/start.sh                 # Prod: Caddy + backend (runs build.sh if artifacts are missing)
bin/start.sh --dev           # Dev: Caddy + cargo-watch + Vite HMR
bin/start.sh --docker        # Caddy + api/portal/valhalla images (docker/docker-compose.yml)

# Engine quality vs Hove (Navitia via PRIM) on random BAN addresses
PRIM_API_KEY=... python3 scripts/compare_engines.py --pairs 100 --seed 42
```

## Architecture

### Module Layout (`src/`)
Domain modules hold the business logic; `api/` is the only HTTP layer. Dependencies flow one way — `api/` → domains → `shared/` — and no domain depends on another.

```
src/
├── main.rs        bootstrap: config load, index build, server wiring, OpenAPI
├── shared/        cross-cutting: config.rs, http.rs, text.rs, util.rs
├── transit/       DOMAIN public transport: gtfs.rs, raptor.rs, realtime/,
│                  disruptions/, validation/
├── geocoding/     DOMAIN addresses: ban.rs
├── traffic/       DOMAIN road traffic: sytadin.rs
└── api/           HTTP layer: journeys/ (public_transport, walk, bike, car,
                   valhalla), places, gtfs, traffic, realtime, disruptions,
                   lines, status, metrics, tiles
```

`realtime/` and `disruptions/` live inside `transit/`: both name stops and lines of the loaded GTFS, and `raptor` reads both overlays at query time. `text.rs` is in `shared/` because `transit::raptor` (stop search) and `geocoding::ban` (address search) both normalize with it. `main.rs` aliases domain entry points (`use transit::{gtfs, raptor, realtime};`).

### Serving
Actix serves the REST API only (:8080) — never the SPA. `bin/start.sh` runs Caddy (`deploy/Caddyfile`, local CA) on `https://portal.glove` (static build, or Vite in dev) and `https://api.glove` (proxy to :8080); `GLOVE_PORTAL_HOST` / `GLOVE_API_HOST` / `GLOVE_HTTPS_PORT` override them. The portal calls the API through `apiUrl()` (`portal/src/api.js`), origin baked from `VITE_API_URL` by `bin/build.sh`; left empty, calls stay same-origin (Vite or nginx proxy `/api`). Cross-origin calls need the portal origin in `server.cors_origins`. Docker images (`docker/`) run non-root with a read-only fs, digest-pinned bases, Trivy-scanned in CI. Env overrides: `GLOVE_VALHALLA_HOST`/`_PORT`, `GLOVE_API_KEY` (the API image blanks the baked key), `GLOVE_TILE_API_KEY`.

### API
Endpoint reference: `/api-docs/openapi.json` (utoipa, generated) and `book/src/api/`. Writes — `POST /api/gtfs/reload` and disruption `POST`/`PUT`/`DELETE` — require the `X-Api-Key` header (`server.api_key`; empty disables them). `/api/status` carries engine health and map defaults only, no GTFS data.

### Frontend (`portal/`)
SPA: 56px nav rail + 450px sidebar + Leaflet map. Views: search, GTFS validation, disruptions back office (`components/DisruptionsPanel.jsx`, lazy, API key in `localStorage`), dataset, swagger, metrics. Queries PT, walk, bike and car in parallel. Two top-right map overlays, each polled only while shown: road traffic and current blockages (`DisruptionLayer`). i18n FR/EN in `i18n.jsx`; pure helpers in `utils.js`, tested with vitest.

### Real-time (`src/transit/realtime/`)
Delays and cancellations applied at query time as an overlay, never by rebuilding the index.
- `model.rs` — connector-agnostic pivot (`TripUpdate`, `StopTimeUpdate`), GTFS-RT vocabulary (SIRI maps onto it, not the reverse)
- `source.rs` — `RealtimeSource` trait; a new format is one file
- `protobuf.rs` — minimal wire-format reader, zero deps (`prost-build` would need `protoc`, absent from CI)
- `gtfs_rt.rs` — GTFS-RT connector; `VehiclePosition`/`Alert` skipped
- `index.rs` — resolves feeds into a `RealtimeIndex` keyed by `(pattern_idx, trip_idx)`
- `service.rs` — one polling task per feed, `ArcSwapOption` publication, per-feed health

Phase 1: delays + cancellations of scheduled trips; `ADDED` trips are counted as unsupported.

### Disruptions (`src/transit/disruptions/`)
Operator-authored works and closures, applied at query time like real-time.
- `model.rs` — `Scope` (`Stop` / `Line` / `LineSection`), `Severity` (`Blocking` / `Info`), `Period` (end absent = ongoing)
- `store.rs` — one JSON document (`{data.dir}/disruptions/disruptions.json`), `ArcSwap` reads, mutex-guarded writes, temp-file + rename
- `overlay.rs` — resolves ids to stop/pattern indices at an instant, maps journeys to the disruptions touching them, builds the map geometry (`blocked_geometry`)

Effects: a blocked *stop* allows no boarding, alighting or transfer (vehicles still pass); a blocked *line* joins `excluded_patterns`; a blocked *section* cuts rides between its endpoints, both directions. `Info` only annotates.

### Key Design Decisions
Each is load-bearing; the book (`book/src/architecture/raptor.md`, `book/src/idfm/engine-comparison.md`) has the measurements behind them.

**Data & concurrency**
- All in-memory, no database. The RAPTOR index is swapped whole through `ArcSwap` on hot-reload; the real-time and disruption overlays are swapped beside it and `RaptorData` is never mutated
- Changing a serialized struct means bumping `CACHE_FORMAT_VERSION` in `raptor.rs`
- Disruptions are the only authored state, hence the only persisted state (a JSON file, not a database)
- One pooled HTTP client per worker thread (`shared/http.rs`) — per thread because a pooled connection is driven by the runtime that opened it
- RAPTOR runs on `web::block`, off the async executor. Its `rounds × stops` buffers are pooled per thread (returned on `RaptorResult` drop, reset sparsely or past `DENSE_RESET_RATIO`); labels store `u32` indices

**RAPTOR correctness**
- `scan_pattern` alights (trip held from an earlier stop) *before* boarding at each stop. Boarding first let a trip "arrive" at its own boarding stop at its arrival time, gaining the dwell and catching vehicles already gone
- Re-boarding at the stop just alighted at costs `routing.default_transfer_time`, or `routing.rail_change_time` between two train/RER/metro vehicles (`QueryOptions::change_times`): IDFM merges a station's platforms into one stop with no transfer time of its own. A run split into two trip ids on one route (terminus → first stop) continues free
- Boarding search uses exact per-position offset bounds (`Pattern::departure_offsets`), not a look-back window; real-time widens them by `PatternDeltas::max_abs_delta`, or a delayed vehicle is invisible
- FIFO boarding pruning (`Pattern::fifo`) is disabled on patterns a real-time overlay touches: delays can reorder vehicles
- Calls are matched by `stop_id`, not `stop_sequence` (`build_patterns` discards sequence values); a forward-only cursor keeps loop routes in order
- Trips that skip a call are set aside like cancellations — re-splitting patterns per refresh is what the overlay exists to avoid
- Queries before `routing.service_day_start` (04:00) run on the previous day's services, +86400 s; disruption periods are resolved against wall-clock time *before* that shift

**Journey search** (`api/journeys/public_transport.rs`)
- Address endpoints: the walk to the `routing.walk_matrix_stops` nearest stops is timed by one Valhalla `sources_to_targets` matrix per endpoint *before* RAPTOR; the others keep the straight-line estimate scaled by the **largest** measured detour. Straight lines undershoot streets by a third or more, which made RAPTOR board unreachable trains
- No journey for an address → one retry within `routing.fallback_stop_distance`
- Iterative diverse search with pattern exclusion; `routing.diverse_lines` excludes the whole head line; `routing.prefer_rail` adds a bus-free first tier, only when a rail/metro/tram stop is within `routing.prefer_rail_max_walk` of both ends
- Routing settings (journeys, transfers, change times, radii, diversity, rail preference, maneuvers) are config-only, never request parameters
- A journey blocked by a disruption is returned with `status: "blocked"` and its causes, found by a second *undisrupted* pass; it never wins a quality tag (`tag_journeys` ranks usable journeys only)
- Closing a stop closes its station (`expand_station` widens to parent and children)

**Other**
- Transfers are always shaped by Valhalla, intra-station ones with zero step/elevator penalties; `routing.maneuvers` only controls whether turn-by-turn steps are attached
- Traffic overlay split by lifetime: Sytadin geometry parsed once (Lambert II → WGS84) and served immutable, cached 24 h; only the states are polled and re-published via `ArcSwapOption`. Both bodies serialized once
- Blockage overlay ships deduplicated, direction-normalized edges (bounded by topology, not pattern count), drawn as straight segments — `shapes.txt` is not loaded
- A real-time feed can answer 200 and match nothing when its ids use another namespace: check `MatchStats` on `/api/realtime/status`

## Configuration

`config.yaml` at repo root is git-ignored; the tracked template is `config.yaml.sample` (copied on first run by `bin/lib/ensure-config.sh`, baked into the API image). **Add every new setting to the sample, to `src/shared/config.rs` with a serde default, and to `book/src/getting-started/configuration.md`.** Unknown keys are silently ignored, so a misspelt key keeps its default. Tunable engine values (speeds, penalties, timeouts, radii, change times) belong in config, not in constants. `FeedConfig`'s `Debug` impl redacts header values and URL query strings so logging the config cannot leak API keys.

## Code Rules

**Rust**
- No `unwrap()` in production code; no silently swallowed errors (log at `warn!`/`debug!`); return `Result` rather than sentinel values
- Functions ~40 lines max, ≤ 3 parameters (bundle related ones in a struct), one level of abstraction
- One term per concept across the codebase (`stop_idx` everywhere)
- Shared helpers: `shared/util.rs` (`parse_coord`, `parse_from_to`, `dir_fingerprint`), Valhalla types and pedestrian costing in `api/journeys/valhalla.rs` — extract anything repeated 3+ times
- `///` doc comments on public items; comments explain why, not what
- GTFS route types (0 tram, 1 metro, 2 rail, 3 bus…) are commented where used

**React**
- Every user-facing string goes through `t()` from `useI18n()`
- `useCallback` on handlers passed to children; every `IconButton` has an `aria-label`; prefer `<button>` over `<div onClick>`
- `JSON.parse` of `localStorage` always in try/catch; fetch `.catch()` logs with `console.warn`
- Next step: split `App.jsx` into `components/` when test coverage allows

## CI

`.github/workflows/ci.yml`, jobs gated by a paths filter: Rust fmt + clippy + test; MSRV `cargo check` on 1.88; `cargo deny check` (`deny.toml`, with a documented GPL-3.0 exception for `actix-governor` pending a maintainer decision); tarpaulin coverage; portal lint + vitest + build + `npm audit`; Docker build and Trivy scan of both images. CI uses the latest stable Rust, so a newer Clippy can flag code that passes locally. Actions are SHA-pinned, kept current by Dependabot. `docs.yml` builds the book on PRs and deploys from master; `docker.yml` publishes images on release. Pre-commit hook: `git config core.hooksPath .githooks`.
