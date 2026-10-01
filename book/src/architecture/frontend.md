# Frontend

The Glove frontend is a single-page React application with an interactive map.

## Technology Stack

| Library | Version | Purpose |
|---------|---------|---------|
| React | 19 | UI framework |
| Vite | 8 | Build tool with HMR |
| MUI (Material-UI) | 9 | Component library (plus `@mui/x-date-pickers`) |
| Leaflet + react-leaflet | 1.9 / 5 | Interactive map |
| Swagger UI React | 5 | API documentation viewer (lazy-loaded) |

Swagger UI is the largest dependency of the bundle, for a view that is rarely opened. It lives in its own module (`portal/src/SwaggerPanel.jsx`) behind `React.lazy`, so its code (about 1.3 MB of JavaScript and 184 kB of CSS, minified) is only fetched when the API view is opened; the initial load carries about 1 MB of JavaScript and 15 kB of CSS. The disruptions back office is split out the same way.

## Source Layout

| File | Content |
|------|---------|
| `portal/src/App.jsx` | Application shell, search form, journey cards, map layers, and the GTFS validation, dataset and metrics views |
| `portal/src/components/DisruptionsPanel.jsx` | Disruptions back office (lazy-loaded) |
| `portal/src/SwaggerPanel.jsx` | Swagger UI view (lazy-loaded) |
| `portal/src/api.js` | `apiUrl()`: prefixes every call with `VITE_API_URL`, baked in at build time (empty = same origin) |
| `portal/src/utils.js` | Pure helpers (time formatting, polyline decoding, mode colors, disruption date conversions), tested with vitest |
| `portal/src/i18n.jsx` | French and English strings, `useI18n()` |

## Layout

The UI consists of three areas:

- **Navigation rail** (56 px, left): the Glove logo returns to the search view; buttons open the **GTFS validation**, **disruptions** and **dataset** views, then the **API** (Swagger) and **metrics** views; the language toggle sits at the bottom
- **Sidebar** (450 px): the active view — by default the search form and journey results
- **Map**: full-height Leaflet map with route visualization and the two overlay toggles (top right)

## Features

### Mode Tabs
Results are grouped into four tabs:
- **Transit** — Public transport via RAPTOR
- **Walk** — Pedestrian routing via Valhalla
- **Bike** — Cycling with 3 profiles (City, E-bike, Road)
- **Car** — Driving via Valhalla

The transit, walk, bike and car requests are sent in parallel; walk, bike and car are only requested when both endpoints have coordinates and the mode is enabled.

### Transport Mode Labels
The frontend displays real commercial names for transit lines rather than generic mode names. For example:
- **RER A** instead of "rail A"
- **Transilien H** instead of "rail H"
- **TER** for regional trains
- **Metro 4** instead of "subway 4"

This provides a familiar experience for users of the Ile-de-France transit network.

### Preferences
A collapsible **Preferences** block in the search form holds three titled sections, each with an icon:
- **Walking Speed** (DirectionsWalk icon) — slider from 2 to 10 km/h, sent as `walking_speed` to the transit and walk requests
- **Transport Modes** (Commute icon) — chips for metro, rail, tramway and bus (unselected ones are sent as `forbidden_modes`), plus walk, bike and car (whether those journeys are requested)
- **Advanced Options** (Tune icon) — the **Wheelchair accessible** switch. When active, the walking speed slider is locked at 3.5 km/h, the bike mode is disabled and not requested, `wheelchair=true` is sent with the transit and walk requests, and the server adds the `most_accessible` journey tag

The walking speed and wheelchair choices are remembered in `localStorage`. Turn-by-turn maneuvers are **server-controlled** (`routing.maneuvers` in `config.yaml`), so there is no client toggle for them.

### Search & Autocomplete
The search form provides:
- Origin and destination fields with fuzzy autocomplete
- Date/time picker
- Swap origin/destination button
- Results appear ranked: stops first, then addresses; recently chosen places are kept in `localStorage`
- A right click on the map sets a point as origin or destination

### Journey Cards
Each transit journey shows its sections, tags and any disruption notices. A journey returned with `status: "blocked"` is shown dimmed, under a red banner listing the blocking disruptions that close it; informational disruptions are listed on the journeys they touch.

### Map Visualization
- Route polylines colored by transport mode
- Stop markers with hover tooltips showing stop names and departure/arrival times
- Origin (green) and destination (red) markers
- Transfer walks drawn from the Valhalla shape, or as a straight line when there is none, with indoor maneuver markers (elevator, stairs, escalator)
- Bike routes colored by elevation gradient (green = descent, red = climb)

### Road Traffic Overlay
A toggle above the map displays live road traffic (see [Road Traffic](../api/traffic.md)). The geometry is fetched once per session and the states are refreshed every minute while the overlay is displayed, then joined by segment id. Traffic events are drawn as dots.

The network holds around 9 000 segments of roughly 4 vertices each, so the rendering cost lies in the number of objects rather than their geometry. Segments are therefore grouped into **one multi-polyline per state** — three Leaflet layers instead of nine thousand — drawn on a canvas renderer, with congestion painted above free-flowing traffic. Simplifying the polylines themselves would gain nothing; Leaflet's own `smoothFactor` handles screen-space reduction.

The overlay is drawn beneath journey polylines, which stay on top.

### Blockage Overlay
A second toggle, left of the traffic one, shows the blocking disruptions in force (`GET /api/disruptions/active`, polled every minute while displayed): closed stops as markers and cut sections as segments, with a tooltip describing each disruption. Segments join consecutive stops in a straight line.

### Disruptions Back Office
The disruptions view (`components/DisruptionsPanel.jsx`) lists, creates, edits and deletes operator disruptions through `/api/disruptions`. Stops and lines are picked with autocomplete (`/api/places`, `/api/lines`). Writes send the `X-Api-Key` header; the key is typed once and kept in `localStorage`.

### GTFS Validation
The GTFS validation view runs `GET /api/gtfs/validate` and displays the data quality report.

### Dataset
The dataset view shows the GTFS statistics (routes, stops, trips, stop times, transfers, calendars, agencies), the RAPTOR statistics (patterns, services) and a button that triggers `POST /api/gtfs/reload`.

### Dark Theme
The app uses a dark theme by default with:
- Map tiles served through the backend's caching proxy (`/api/tiles`), from `map.tile_url` — CARTO Voyager by default — slightly desaturated in CSS
- Glassmorphism UI effects (translucent sidebar)
- MUI dark palette

### Internationalization
Two languages are supported via `portal/src/i18n.jsx`:
- **French**
- **English**

The initial language is French when the browser's locale starts with `fr`, English otherwise. A button in the navigation rail switches it for the session (the choice is not persisted). The language is also sent to the API so Valhalla instructions come back in it.

### Metrics
The metrics view shows live server statistics parsed from `GET /api/metrics`:
- Process: CPU time, resident and virtual memory, open file descriptors, threads, uptime
- HTTP: total requests and total errors

### Map Bounds
The map is constrained to the configured geographic bounds (`map.bounds_sw_lat` / `bounds_sw_lon` / `bounds_ne_lat` / `bounds_ne_lon`, served by `/api/status`; Ile-de-France by default) to prevent users from searching outside the coverage area.
