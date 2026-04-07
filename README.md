# Glove

[![CI](https://github.com/ltoinel/Glove/actions/workflows/ci.yml/badge.svg)](https://github.com/ltoinel/Glove/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/ltoinel/Glove/graph/badge.svg)](https://codecov.io/gh/ltoinel/Glove)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB.svg)](https://react.dev/)
[![GTFS](https://img.shields.io/badge/GTFS-Île--de--France-green.svg)](https://data.iledefrance-mobilites.fr/)

A fast **multi-modal journey planner** built in Rust with a React frontend.

Glove loads GTFS data into memory, builds a RAPTOR index, and exposes a Navitia-compatible REST API for journey planning. It supports public transit, walking, cycling, and driving via [Valhalla](https://github.com/valhalla/valhalla) integration. The React portal provides an interactive map-based interface with autocomplete, route visualization, and multilingual support (FR/EN).

![Glove screenshot](docs/screenshot.jpg)

## Features

- **RAPTOR algorithm** — Round-based public transit routing with diverse alternatives
- **Multi-modal** — Public transit, walking, cycling (3 bike profiles), driving
- **Fuzzy autocomplete** — Stop and address search with French diacritics normalization
- **Hot reload** — Update GTFS data via API without downtime (lock-free with ArcSwap)
- **Interactive map** — Leaflet with route polylines, elevation-colored bike routes, dark theme
- **Navitia-compatible API** — Drop-in replacement with OpenAPI documentation
- **Prometheus metrics** — Built-in monitoring endpoint

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
