# Project Structure

```
Glove/
├── src/                         # Rust backend (api/ → domains → shared/)
│   ├── main.rs                  # Bootstrap: config, index build, server wiring, metrics middleware, OpenAPI
│   ├── shared/                  # Cross-cutting
│   │   ├── config.rs            # Nested YAML configuration with defaults
│   │   ├── http.rs              # Pooled outbound HTTP client (one per worker thread)
│   │   ├── text.rs              # Text normalization (diacritics)
│   │   └── util.rs              # parse_coord, parse_from_to, dir_fingerprint
│   ├── transit/                 # DOMAIN public transport
│   │   ├── gtfs.rs              # GTFS data model & CSV loader
│   │   ├── raptor.rs            # RAPTOR algorithm & index building
│   │   ├── realtime/            # Real-time overlay: model, source, protobuf,
│   │   │                        #   gtfs_rt, index, service
│   │   ├── disruptions/         # Operator-authored disruptions: model, store, overlay
│   │   └── validation/          # GTFS validation rules: integrity, calendar,
│   │                            #   schedule, network (+ tests)
│   ├── geocoding/
│   │   └── ban.rs               # BAN address geocoding
│   ├── traffic/
│   │   └── sytadin.rs           # Sytadin geometry (MIF/MID, reprojection) & live feed parsing
│   └── api/                     # HTTP layer
│       ├── mod.rs               # Shared response types
│       ├── journeys/
│       │   ├── mod.rs           # Journey module entry
│       │   ├── public_transport.rs  # RAPTOR journey planning
│       │   ├── walk.rs          # Walking via Valhalla
│       │   ├── bike.rs          # Cycling (3 profiles) via Valhalla
│       │   ├── car.rs           # Driving via Valhalla
│       │   └── valhalla.rs      # Shared Valhalla types and calls
│       ├── places.rs            # Autocomplete (stops + addresses)
│       ├── gtfs.rs              # GTFS status, validation & reload endpoints
│       ├── lines.rs             # Line catalogue for the back-office pickers
│       ├── disruptions.rs       # Disruption CRUD + active blockages
│       ├── realtime.rs          # Real-time feed health
│       ├── tiles.rs             # Map tile proxy with disk cache
│       ├── metrics.rs           # Prometheus metrics endpoint
│       ├── traffic.rs           # Road traffic endpoints & refresh loop
│       └── status.rs            # Status endpoint
│
├── portal/                      # React frontend
│   ├── src/
│   │   ├── App.jsx              # Main SPA (search, results, map, metrics)
│   │   ├── components/
│   │   │   └── DisruptionsPanel.jsx # Disruptions back office, lazy-loaded
│   │   ├── api.js               # API base URL (VITE_API_URL) — apiUrl() for every call
│   │   ├── SwaggerPanel.jsx     # API docs view, lazy-loaded
│   │   ├── i18n.jsx             # Internationalization (FR/EN)
│   │   ├── main.jsx             # Entry point with MUI theme
│   │   ├── index.css            # Styling
│   │   ├── utils.js             # Pure utility functions (tested with vitest)
│   │   └── test/                # Vitest test files
│   ├── package.json
│   ├── vite.config.js
│   └── eslint.config.js
│
├── bin/                         # Utility scripts
│   ├── build.sh                 # Release build: backend binary + portal SPA
│   ├── start.sh                 # Start script (production & dev), behind Caddy
│   ├── download.sh              # Data download (GTFS, OSM, BAN, traffic)
│   ├── valhalla.sh              # Valhalla Docker setup
│   └── lib/ensure-config.sh     # Copies config.yaml.sample to config.yaml on first run
│
├── scripts/                     # Analysis & benchmarking
│   ├── benchmark.py             # Performance benchmark with charts
│   ├── compare_engines.py       # Journey quality vs other engines (Hove/Navitia) on random BAN addresses
│   ├── check_indoor.py          # Check GTFS transfers for indoor routing data
│   ├── pathway_valhalla_diff.py # GTFS pathway times vs Valhalla indoor walks (CSV)
│   └── gen_pathway_gaps_page.py # Book page + chart from that CSV
│
├── deploy/
│   └── Caddyfile                # HTTPS reverse proxy: portal.glove + api.glove (bin/start.sh)
│
├── docker/
│   ├── Dockerfile.api           # API image (Rust build + Debian slim runtime)
│   ├── Dockerfile.portal        # Portal image (Vite build + nginx)
│   ├── nginx.conf               # Portal nginx: static SPA + /api proxy to the api service
│   └── docker-compose.yml       # api + portal + valhalla
│
├── book/                        # Documentation (mdBook)
│   ├── book.toml
│   └── src/
│       └── images/              # Documentation images (screenshots, benchmarks)
│
├── data/                        # Data files (not committed)
│   ├── gtfs/                    # GTFS transit schedules
│   ├── osm/                     # OpenStreetMap data
│   ├── raptor/                  # Serialized RAPTOR index cache
│   ├── ban/                     # French address data
│   ├── tiles/                   # Cached map tiles (auto-populated)
│   ├── sytadin/                 # Road network geometry (traffic overlay)
│   ├── disruptions/             # Disruption catalog (disruptions.json, authored)
│   └── valhalla/                # Valhalla routing tiles
│
├── config.yaml.sample           # Configuration template (copy to config.yaml, git-ignored)
├── Cargo.toml                   # Rust dependencies
├── CLAUDE.md                    # AI assistant guidance
├── README.md                    # Project overview
├── LICENSE.md                   # MIT license
├── SECURITY.md                  # Security policy
├── deny.toml                    # cargo-deny: advisories, licenses, sources
├── clippy.toml                  # Clippy settings
├── .dockerignore                # Docker build context filter
├── .githooks/pre-commit         # Local lint hook (git config core.hooksPath .githooks)
└── .github/
    ├── workflows/ci.yml         # CI: lint, tests, MSRV, audit, coverage, Docker build
    ├── workflows/docs.yml       # Book build (PRs) and GitHub Pages deploy (master)
    ├── workflows/docker.yml     # Publishes the API and portal images on release
    └── dependabot.yml           # Weekly dependency updates
```

## Key Files

| File | Lines | Description |
|------|-------|-------------|
| `src/api/journeys/public_transport.rs` | ~3,800 | Journey planning endpoint and response formatting |
| `src/transit/raptor.rs` | ~3,600 | RAPTOR algorithm, the core of the application |
| `portal/src/App.jsx` | ~2,700 | Frontend SPA (search, results, map, metrics) |
| `src/shared/config.rs` | ~1,230 | Configuration with defaults (server, routing, map, bike, wheelchair, traffic, realtime) |
| `src/api/disruptions.rs` | ~840 | Disruption CRUD and active-blockage endpoints |
| `src/geocoding/ban.rs` | ~730 | BAN address geocoding with number interpolation |
| `src/traffic/sytadin.rs` | ~720 | Sytadin road geometry (MIF/MID, reprojection) and live feed parsing |
| `src/transit/gtfs.rs` | ~720 | GTFS CSV parsing and data model |
| `src/transit/realtime/index.rs` | ~720 | Real-time feeds resolved into a RAPTOR overlay |
| `src/transit/disruptions/overlay.rs` | ~660 | Disruptions resolved into stop/pattern exclusions and map geometry |
| `src/transit/validation/` | ~2,240 (6 files) | GTFS validation rule catalogue |
| `src/api/traffic.rs` | ~510 | Traffic geometry/states endpoints and background refresh loop |
| `src/api/gtfs.rs` | ~490 | GTFS status, validation and reload endpoints |
| `src/api/places.rs` | ~410 | Fuzzy search with ranking |
| `src/api/tiles.rs` | ~380 | Map tile proxy with disk cache |
| `src/api/metrics.rs` | ~370 | Prometheus metrics collection |
