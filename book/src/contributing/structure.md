# Project Structure

```
Glove/
├── src/                         # Rust backend
│   ├── main.rs                  # Entry point, server setup, metrics middleware
│   ├── config.rs                # Nested YAML configuration
│   ├── gtfs.rs                  # GTFS data model & CSV loader
│   ├── raptor.rs                # RAPTOR algorithm & index building
│   ├── ban.rs                   # BAN address geocoding
│   ├── text.rs                  # Text normalization (diacritics)
│   └── api/
│       ├── mod.rs               # Shared response types
│       ├── journeys/
│       │   ├── mod.rs           # Journey module entry
│       │   ├── public_transport.rs  # RAPTOR journey planning
│       │   ├── walk.rs          # Walking via Valhalla
│       │   ├── bike.rs          # Cycling (3 profiles) via Valhalla
│       │   ├── car.rs           # Driving via Valhalla
│       │   └── valhalla.rs      # Shared Valhalla HTTP client
│       ├── places.rs            # Autocomplete (stops + addresses)
│       ├── gtfs.rs              # GTFS validation & reload endpoints
│       ├── tiles.rs             # Map tile proxy with disk cache
│       ├── metrics.rs           # Prometheus metrics endpoint
│       └── status.rs            # Status endpoint
│
├── portal/                      # React frontend
│   ├── src/
│   │   ├── App.jsx              # Main SPA (search, results, map, metrics)
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
│   ├── start.sh                 # Start script (production & dev)
│   ├── download.sh              # Data download (GTFS, OSM, BAN)
│   └── valhalla.sh              # Valhalla Docker setup
│
├── scripts/                     # Analysis & benchmarking
│   ├── benchmark.py             # Performance benchmark with charts
│   └── check_indoor.py          # Check GTFS transfers for indoor routing data
│
├── docker/
│   └── Dockerfile               # Multi-stage build (Node + Rust + Debian)
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
│   └── valhalla/                # Valhalla routing tiles
│
├── config.yaml                  # Application configuration
├── Cargo.toml                   # Rust dependencies
├── CLAUDE.md                    # AI assistant guidance
├── README.md                    # Project overview
├── LICENSE.md                   # MIT license
└── .github/workflows/ci.yml    # CI pipeline
```

## Key Files

| File | Lines | Description |
|------|-------|-------------|
| `src/raptor.rs` | ~2,200 | RAPTOR algorithm, the core of the application |
| `portal/src/App.jsx` | ~2,200 | Entire frontend SPA in one file |
| `src/api/journeys/public_transport.rs` | ~1,450 | Journey planning endpoint and response formatting |
| `src/api/gtfs.rs` | ~830 | GTFS validation (19 checks) & reload endpoint |
| `src/config.rs` | ~750 | Configuration with defaults (server, routing, map, bike, wheelchair) |
| `src/ban.rs` | ~630 | BAN address geocoding with number interpolation |
| `src/gtfs.rs` | ~570 | GTFS CSV parsing and data model |
| `src/api/places.rs` | ~340 | Fuzzy search with ranking |
| `src/api/metrics.rs` | ~370 | Prometheus metrics collection |
| `src/api/tiles.rs` | ~110 | Map tile proxy with disk cache |
