# Frontend

The Glove frontend is a single-page React application with an interactive map.

## Technology Stack

| Library | Version | Purpose |
|---------|---------|---------|
| React | 19 | UI framework |
| Vite | - | Build tool with HMR |
| MUI (Material-UI) | 7 | Component library |
| Leaflet + react-leaflet | - | Interactive map |
| Swagger UI React | - | API documentation viewer |

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
- **Advanced Options** (Tune icon) — includes a "Show detailed instructions" switch ("Afficher les instructions detaillees") that enables turn-by-turn maneuvers in journey responses. This switch is disabled by default; when toggled on, the `maneuvers=true` parameter is sent to all journey endpoints.

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
