# Glove

[![CI](https://github.com/ltoinel/Glove/actions/workflows/ci.yml/badge.svg)](https://github.com/ltoinel/Glove/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/ltoinel/Glove/graph/badge.svg)](https://codecov.io/gh/ltoinel/Glove)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/ltoinel/Glove/blob/master/LICENSE.md)

A fast **multi-modal journey planner** built in Rust with a React frontend.

![Glove screenshot](./images/screenshot.jpg)

Glove loads [GTFS](https://gtfs.org/) data into memory, builds a [RAPTOR](https://www.microsoft.com/en-us/research/wp-content/uploads/2012/01/raptor_alenex.pdf) index, and exposes a **Navitia-compatible REST API** for journey planning. It supports public transit, walking, cycling, and driving via [Valhalla](https://github.com/valhalla/valhalla) integration.

The React portal provides an interactive map-based interface with autocomplete, route visualization, and multilingual support (FR/EN).

```admonish tip title="Quick Start"
Get up and running in 3 commands:

~~~bash
bin/download.sh all      # Download data
bin/valhalla.sh          # Start Valhalla (optional)
bin/start.sh             # Start the server
~~~

Then open [http://localhost:8080](http://localhost:8080)
```

## Key Features

### Routing
- **RAPTOR algorithm** for optimal public transit journey computation
- **Multi-modal**: public transit, walking, cycling (3 profiles), driving
- **Diverse alternatives** with progressive pattern exclusion
- **Journey tags**: *fastest*, *least transfers*, *least walking*
- **Elevation-colored bike routes** (green = descent, red = climb)
- **Turn-by-turn directions** for walk, bike, and car routes

### Data & Search
- **Fuzzy autocomplete** with French diacritics normalization
- **BAN integration** for French address geocoding
- **Hot reload** via API without service interruption

### Frontend
- **Interactive Leaflet map** with route polylines and stop markers
- **Mode tabs**: Transit, Walk, Bike, Car
- **Dark theme** with CARTO basemap and glassmorphism UI
- **Metrics panel** with live CPU, memory, and request stats

### Developer Experience
- **Navitia-compatible API** for drop-in replacement
- **OpenAPI documentation** auto-generated
- **Prometheus metrics** endpoint
- **Benchmark tool** for load testing
- **Dev mode** with cargo-watch + Vite HMR
