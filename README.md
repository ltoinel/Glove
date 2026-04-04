# Glove

[![CI](https://github.com/ltoinel/Glove/actions/workflows/ci.yml/badge.svg)](https://github.com/ltoinel/Glove/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB.svg)](https://react.dev/)
[![GTFS](https://img.shields.io/badge/GTFS-Île--de--France-green.svg)](https://data.iledefrance-mobilites.fr/)

A fast **multi-modal journey planner** for Île-de-France, built in Rust with a React frontend.

Glove loads GTFS data into memory, builds a RAPTOR index, and exposes a Navitia-compatible REST API for journey planning. It supports public transit, walking, cycling, and driving via [Valhalla](https://github.com/valhalla/valhalla) integration. The React portal provides an interactive map-based interface with autocomplete, route visualization, and multilingual support (FR/EN).

![Glove screenshot](docs/screenshot.jpg)

## Features

### Routing
- **RAPTOR algorithm** — Round-based Public Transit Routing for optimal journey computation
- **Multi-modal** — Public transit, walking, cycling (standard + e-bike), and driving via Valhalla
- **Diverse alternatives** — Returns up to N different route options by progressively excluding used patterns
- **Journey tags** — Automatically labels journeys: *fastest*, *least transfers*, *least walking*
- **Elevation tracking** — Elevation gain/loss for bike routes via Valhalla height API
- **Turn-by-turn** — Maneuver-by-maneuver directions for walk, bike, and car routes

### Data & Search
- **Autocomplete** — Fuzzy stop and address search with French diacritics normalization
- **BAN integration** — French address geocoding from Base Adresse Nationale data
- **SIRENE database** — Company directory download support
- **Hot reload** — Reload GTFS data via API without service interruption (lock-free with ArcSwap)

### Frontend
- **Interactive map** — Leaflet map with route polylines, stop markers, origin/destination flags
- **Mode tabs** — Switch between Transit, Walk, Bike, and Car
- **Multilingual UI** — French and English, auto-detected from browser
- **Dark theme** — CARTO dark basemap with glassmorphism UI

### Developer experience
- **Navitia-compatible API** — Drop-in replacement for Navitia query parameters
- **OpenAPI documentation** — Auto-generated spec served at `/api-docs/openapi.json`
- **Dev mode** — `cargo-watch` + Vite HMR for fast iteration
- **YAML configuration** — All parameters configurable via `config.yaml`
- **Structured logging** — Tracing with configurable log levels

## Quick start

### Prerequisites

- [Rust](https://rustup.rs/) (1.75+)
- [Node.js](https://nodejs.org/) (18+)
- [Docker](https://www.docker.com/) (for Valhalla, optional)

### Download data

```bash
bin/download.sh all      # Download GTFS, OSM, BAN and SIRENE data
bin/download.sh gtfs     # GTFS only
bin/download.sh osm      # OSM only
bin/download.sh ban      # BAN addresses only
bin/download.sh sirene   # SIRENE company database only
```

### Start Valhalla (optional, for walk/bike/car routing)

```bash
bin/valhalla.sh          # Pulls Docker image, builds tiles, starts on port 8002
```

### Run

```bash
bin/start.sh             # Production: builds and starts everything
bin/start.sh --dev       # Dev: cargo-watch + Vite HMR
```

- **API**: http://localhost:8080
- **Portal**: http://localhost:3000 (dev mode)

### Manual start

```bash
# Backend
cargo run --release

# Frontend (in another terminal)
cd portal && npm install && npm run dev
```

## Configuration

All settings are in `config.yaml`:

```yaml
# Server
bind: "0.0.0.0"
port: 8080
workers: 0                   # 0 = auto (one per CPU)
log_level: "info"

# Data directories
data_dir: "data/gtfs"
osm_dir: "data/osm"
ban_dir: "data/ban"
sirene_dir: "data/sirene"

# Routing
max_journeys: 5
max_transfers: 5
default_transfer_time: 120   # seconds
max_duration: 10800          # 3 hours

# Valhalla (walk/bike/car routing)
valhalla_host: "localhost"
valhalla_port: 8002

# Map defaults
map_center_lat: 48.8566
map_center_lon: 2.3522
map_zoom: 11
```

## API

### `GET /api/journeys/public_transport`

Compute public transit journey alternatives between two stops.

```
GET /api/journeys/public_transport?from=IDFM:22101&to=IDFM:21966&datetime=20260404T083000
```

Key parameters: `from`, `to`, `datetime`, `max_nb_transfers`, `max_duration`, `count`.

### `GET /api/journeys/walk`

Walking directions between two coordinates via Valhalla.

```
GET /api/journeys/walk?from=2.3522;48.8566&to=2.3488;48.8534
```

### `GET /api/journeys/bike`

Cycling directions (standard bike + e-bike variants) with elevation data.

```
GET /api/journeys/bike?from=2.3522;48.8566&to=2.3488;48.8534
```

### `GET /api/journeys/car`

Driving directions via Valhalla.

```
GET /api/journeys/car?from=2.3522;48.8566&to=2.3488;48.8534
```

### `GET /api/places`

Stop and address autocomplete with fuzzy search.

```
GET /api/places?q=chatelet&limit=5
```

### `GET /api/status`

Engine status and GTFS data statistics.

### `POST /api/reload`

Hot-reload GTFS data without downtime.

### `GET /api-docs/openapi.json`

OpenAPI 3.0 specification.

## Project structure

```
Glove/
├── src/
│   ├── main.rs              # Entry point, server setup
│   ├── config.rs            # YAML configuration
│   ├── gtfs.rs              # GTFS data model & CSV loader
│   ├── raptor.rs            # RAPTOR algorithm & index
│   ├── ban.rs               # BAN address geocoding
│   ├── text.rs              # Text normalization (diacritics)
│   └── api/
│       ├── mod.rs           # Shared response types
│       ├── journeys/
│       │   ├── mod.rs       # Journey module
│       │   ├── public_transport.rs
│       │   ├── walk.rs
│       │   ├── bike.rs
│       │   └── car.rs
│       ├── places.rs        # Autocomplete endpoint
│       └── status.rs        # Status & reload endpoints
├── portal/                  # React frontend (Vite + MUI + Leaflet)
│   ├── src/
│   │   ├── App.jsx          # Main application component
│   │   ├── i18n.jsx         # Internationalization (FR/EN)
│   │   └── main.jsx         # Entry point with MUI theme
│   └── package.json
├── bin/
│   ├── start.sh             # Start script (production & dev)
│   ├── download.sh          # Data download (GTFS, OSM, BAN, SIRENE)
│   └── valhalla.sh          # Valhalla Docker setup
├── config.yaml              # Application configuration
└── data/                    # Data files (not committed)
    ├── gtfs/
    ├── osm/
    ├── ban/
    └── sirene/
```

## License

[MIT](LICENSE.md)
