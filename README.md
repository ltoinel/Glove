# Glove

[![CI](https://github.com/ltoinel/Glove/actions/workflows/ci.yml/badge.svg)](https://github.com/ltoinel/Glove/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/ltoinel/Glove/graph/badge.svg)](https://codecov.io/gh/ltoinel/Glove)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB.svg)](https://react.dev/)
[![GTFS](https://img.shields.io/badge/GTFS-Île--de--France-green.svg)](https://data.iledefrance-mobilites.fr/)

A fast **multi-modal journey planner** built in Rust with a React frontend.

Glove loads GTFS data into memory, builds a RAPTOR index, and exposes a Navitia-compatible REST API for journey planning. It supports public transit, walking, cycling, and driving via [Valhalla](https://github.com/valhalla/valhalla) integration. The React portal provides an interactive map-based interface with autocomplete, route visualization, and multilingual support (FR/EN).

![Glove screenshot](book/src/images/screenshot.jpg)

## Features

### Routing
- **RAPTOR algorithm** — Round-based public transit routing with diverse alternatives
- **Multi-modal** — Public transit, walking, cycling (3 bike profiles), driving
- **Indoor-aware transfers** — Valhalla pedestrian routing with zero step/elevator penalties for intra-station walks, favoring underground passages
- **Station-aware resolution** — Stop IDs resolve to all platforms via parent_station, enabling correct routing through large station complexes (metro, RER, TGV)
- **After-midnight routing** — Queries before 4am automatically use previous day GTFS services for night buses and late trains

### Data & Search
- **Fuzzy autocomplete** — Stop and address search with French diacritics normalization
- **GTFS validation** — 19 automated data quality checks (referential integrity, calendars, coordinates, transfers, pathways, display)
- **Hot reload** — Update GTFS data via API without downtime (lock-free with ArcSwap)
- **Tile caching proxy** — Map tiles fetched from configurable upstream server, cached locally on disk

### Frontend
- **Interactive map** — Leaflet with route polylines, elevation-colored bike routes, indoor/outdoor transfer distinction, dark theme
- **Vertical nav rail** — Quick access to search, GTFS validation, dataset info, API docs, metrics
- **Multilingual** — French and English (i18n)

### API & Operations
- **Navitia-compatible API** — Drop-in replacement with OpenAPI documentation
- **Prometheus metrics** — Built-in monitoring endpoint (CPU, memory, HTTP counters)
- **Rate limiting** — Configurable per-IP rate limiting (tile proxy excluded)
- **Clean Code** — Rust backend with 179 tests, React frontend with 21 vitest tests

## Quick Start

```bash
bin/download.sh all      # Download GTFS, OSM and BAN data
bin/valhalla.sh          # Start Valhalla (optional, for walk/bike/car)
bin/start.sh             # Production: builds and starts everything
bin/start.sh --dev       # Dev: cargo-watch + Vite HMR
```

- **Portal**: [http://localhost:8080](http://localhost:8080) (production) / [http://localhost:3000](http://localhost:3000) (dev)
- **API**: [http://localhost:8080/api](http://localhost:8080/api)

## Documentation

Full documentation is available at **[ltoinel.github.io/Glove](https://ltoinel.github.io/Glove/)**, covering:

- [Installation & Configuration](https://ltoinel.github.io/Glove/getting-started/installation.html)
- [Architecture & RAPTOR Algorithm](https://ltoinel.github.io/Glove/architecture/raptor.html)
- [API Reference](https://ltoinel.github.io/Glove/api/endpoints.html)
- [Performance & Monitoring](https://ltoinel.github.io/Glove/operations/performance.html)
- [Contributing](https://ltoinel.github.io/Glove/contributing/development.html)

## License

[MIT](LICENSE.md)
