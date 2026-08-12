# Frontend

The Glove frontend is a single-page React application with an interactive map.

## Technology Stack

| Library | Version | Purpose |
|---------|---------|---------|
| React | 19 | UI framework |
| Vite | - | Build tool with HMR |
| MUI (Material-UI) | 7 | Component library |
| Leaflet + react-leaflet | - | Interactive map |
| Swagger UI React | - | API documentation viewer (lazy-loaded) |

Swagger UI accounts for more than half the compiled bundle, for a view that is rarely opened. It lives in its own module (`portal/src/SwaggerPanel.jsx`) behind `React.lazy`, so the initial load carries 979 kB of JS instead of 2 270 kB, and 15 kB of CSS instead of 198 kB.

## Layout

The UI consists of two main areas:

- **Left sidebar**: search form, journey results, settings panel, metrics
- **Right area**: full-height Leaflet map with route visualization

All components live in a single file `portal/src/App.jsx` for simplicity.

## Features

### Mode Tabs
Four transport modes are available as tabs:
- **Transit** — Public transport via RAPTOR
- **Walk** — Pedestrian routing via Valhalla
- **Bike** — Cycling with 3 profiles (City, E-bike, Road)
- **Car** — Driving via Valhalla

Transit and Walk/Bike/Car queries are sent in parallel; results are displayed as they arrive.

### Transport Mode Labels
The frontend displays real commercial names for transit lines rather than generic mode names. For example:
- **RER A** instead of "rail A"
- **Transilien H** instead of "rail H"
- **TER** for regional trains
- **Metro 4** instead of "subway 4"

This provides a familiar experience for users of the Ile-de-France transit network.

### Settings Panel
The settings panel is organized into three titled sections, each with an icon:
- **Walking Speed** (DirectionsWalk icon) — adjusts walking speed for transit journey calculations
- **Transport Modes** (Commute icon) — select which transit modes to include
- **Advanced Options** (Tune icon) — includes:
  - **Wheelchair accessible** switch — enables wheelchair-accessible routing. When active, walking speed is locked to 3.5 km/h, bike and car modes are hidden, and the `most_accessible` journey tag is displayed

Turn-by-turn maneuvers are **server-controlled** (`routing.maneuvers` in `config.yaml`), so there is no client toggle for them.

### Search & Autocomplete
The search form provides:
- Origin and destination fields with fuzzy autocomplete
- Date/time picker
- Swap origin/destination button
- Results appear ranked: stops first, then addresses

### Map Visualization
- Route polylines colored by transport mode
- Stop markers with popups showing stop names and departure/arrival times
- Origin (green) and destination (red) bubbles
- Bike routes colored by elevation gradient (green = descent, red = climb)

### Road Traffic Overlay
A toggle above the map displays live road traffic (see [Road Traffic](../api/traffic.md)). The geometry is fetched once per session and the states are refreshed on a timer, then joined by segment id.

The network holds around 9 000 segments of roughly 4 vertices each, so the rendering cost lies in the number of objects rather than their geometry. Segments are therefore grouped into **one multi-polyline per state** — three Leaflet layers instead of nine thousand — drawn on a canvas renderer, with congestion painted above free-flowing traffic. Simplifying the polylines themselves would gain nothing; Leaflet's own `smoothFactor` handles screen-space reduction.

The overlay is drawn beneath journey polylines, which stay on top.

### Dark Theme
The app uses a dark theme by default with:
- CARTO Dark Matter basemap tiles
- Glassmorphism UI effects (translucent sidebar)
- MUI dark palette

### Internationalization
Two languages are supported via `portal/src/i18n.jsx`:
- **French** (default, auto-detected)
- **English**

The language is detected from the browser's locale and can be toggled in the UI.

### Metrics Panel
A collapsible metrics panel shows live server statistics:
- CPU and memory usage
- Uptime
- HTTP request counts and error rates
- GTFS data stats (stops, routes, trips)

### Map Bounds
The map is constrained to the configured geographic bounds (default: Ile-de-France) to prevent users from searching outside the coverage area.
