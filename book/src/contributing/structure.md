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
│       ├── metrics.rs           # Prometheus metrics endpoint
│       └── status.rs            # Status & reload endpoints
│
├── portal/                      # React frontend
│   ├── src/
│   │   ├── App.jsx              # Main SPA (search, results, map, metrics)
│   │   ├── i18n.jsx             # Internationalization (FR/EN)
│   │   ├── main.jsx             # Entry point with MUI theme
│   │   └── index.css            # Styling
│   ├── package.json
│   ├── vite.config.js
│   └── eslint.config.js
│
├── bin/                         # Utility scripts
│   ├── start.sh                 # Start script (production & dev)
│   ├── download.sh              # Data download (GTFS, OSM, BAN)
│   ├── valhalla.sh              # Valhalla Docker setup
│   └── benchmark.py             # Performance benchmark with charts
│
├── docker/
│   └── Dockerfile               # Multi-stage build (Node + Rust + Debian)
│
├── book/                        # Documentation (mdBook)
│   ├── book.toml
│   └── src/
│
├── data/                        # Data files (not committed)
│   ├── gtfs/                    # GTFS transit schedules
│   ├── osm/                     # OpenStreetMap data
│   ├── raptor/                  # Serialized RAPTOR index cache
│   ├── ban/                     # French address data
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
| `src/raptor.rs` | ~2,100 | RAPTOR algorithm, the core of the application |
| `portal/src/App.jsx` | ~1,065 | Entire frontend SPA in one file |
| `src/api/journeys/public_transport.rs` | ~900 | Journey planning endpoint and response formatting |
| `src/api/places.rs` | ~300 | Fuzzy search with ranking |
| `src/api/metrics.rs` | ~280 | Prometheus metrics collection |
| `src/gtfs.rs` | ~500 | GTFS CSV parsing and data model |
| `src/config.rs` | ~150 | Configuration with defaults |
