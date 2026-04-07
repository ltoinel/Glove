# Architecture Overview

Glove is a monorepo with a Rust backend and React frontend.

## High-Level Architecture

<svg viewBox="0 0 720 480" xmlns="http://www.w3.org/2000/svg" style="max-width:720px;width:100%;font-family:'DM Sans',sans-serif;">
  <defs>
    <filter id="glow"><feGaussianBlur stdDeviation="2" result="g"/><feMerge><feMergeNode in="g"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
    <linearGradient id="cyan-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#00e5ff" stop-opacity="0.15"/><stop offset="100%" stop-color="#00e5ff" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="amber-grad" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#ffb800" stop-opacity="0.15"/><stop offset="100%" stop-color="#ffb800" stop-opacity="0.05"/></linearGradient>
  </defs>
  <!-- Frontend -->
  <rect x="160" y="10" width="400" height="60" rx="10" fill="url(#cyan-grad)" stroke="#00e5ff" stroke-width="1.5"/>
  <text x="360" y="35" text-anchor="middle" fill="#00e5ff" font-size="14" font-weight="700">Frontend (React)</text>
  <text x="360" y="55" text-anchor="middle" fill="#8b89a0" font-size="11">Leaflet Map · MUI Sidebar · i18n</text>
  <!-- Arrow -->
  <line x1="360" y1="70" x2="360" y2="100" stroke="#8b89a0" stroke-width="1.5" marker-end="url(#arrow)"/>
  <text x="380" y="90" fill="#56546a" font-size="9">HTTP / JSON</text>
  <defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#8b89a0"/></marker></defs>
  <!-- Server box -->
  <rect x="40" y="100" width="640" height="360" rx="12" fill="rgba(20,20,35,0.5)" stroke="rgba(255,255,255,0.09)" stroke-width="1.5"/>
  <text x="360" y="125" text-anchor="middle" fill="#e4e2ec" font-size="13" font-weight="700">Actix-web Server</text>
  <text x="360" y="142" text-anchor="middle" fill="#56546a" font-size="10">CORS · Rate Limiting · Metrics Middleware</text>
  <!-- API endpoints row -->
  <rect x="70" y="160" width="140" height="50" rx="8" fill="url(#cyan-grad)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="140" y="182" text-anchor="middle" fill="#00e5ff" font-size="11" font-weight="600">/journeys</text>
  <text x="140" y="198" text-anchor="middle" fill="#56546a" font-size="9">/walk /bike /car</text>
  <rect x="230" y="160" width="120" height="50" rx="8" fill="url(#cyan-grad)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="290" y="190" text-anchor="middle" fill="#00e5ff" font-size="11" font-weight="600">/places</text>
  <rect x="370" y="160" width="120" height="50" rx="8" fill="url(#cyan-grad)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="430" y="182" text-anchor="middle" fill="#00e5ff" font-size="11" font-weight="600">/status</text>
  <text x="430" y="198" text-anchor="middle" fill="#56546a" font-size="9">/reload</text>
  <rect x="510" y="160" width="120" height="50" rx="8" fill="url(#cyan-grad)" stroke="#00e5ff" stroke-opacity="0.4" stroke-width="1"/>
  <text x="570" y="190" text-anchor="middle" fill="#00e5ff" font-size="11" font-weight="600">/metrics</text>
  <!-- RAPTOR Index -->
  <rect x="80" y="250" width="200" height="70" rx="10" fill="url(#amber-grad)" stroke="#ffb800" stroke-opacity="0.5" stroke-width="1.5"/>
  <text x="180" y="280" text-anchor="middle" fill="#ffb800" font-size="13" font-weight="700">RAPTOR Index</text>
  <text x="180" y="300" text-anchor="middle" fill="#8b89a0" font-size="10">ArcSwap · Lock-free</text>
  <!-- BAN Index -->
  <rect x="320" y="250" width="160" height="70" rx="10" fill="url(#cyan-grad)" stroke="#00e5ff" stroke-opacity="0.3" stroke-width="1"/>
  <text x="400" y="280" text-anchor="middle" fill="#e4e2ec" font-size="12" font-weight="600">BAN Index</text>
  <text x="400" y="298" text-anchor="middle" fill="#8b89a0" font-size="10">Addresses</text>
  <!-- Arrow RAPTOR to GTFS -->
  <line x1="180" y1="320" x2="180" y2="355" stroke="#8b89a0" stroke-width="1" marker-end="url(#arrow)"/>
  <!-- GTFS Data -->
  <rect x="80" y="360" width="200" height="60" rx="10" fill="rgba(255,255,255,0.03)" stroke="rgba(255,255,255,0.09)" stroke-width="1"/>
  <text x="180" y="386" text-anchor="middle" fill="#e4e2ec" font-size="12" font-weight="600">GTFS Data</text>
  <text x="180" y="404" text-anchor="middle" fill="#56546a" font-size="10">CSV files</text>
  <!-- Valhalla -->
  <rect x="440" y="360" width="200" height="60" rx="10" fill="rgba(0,230,118,0.08)" stroke="#00e676" stroke-opacity="0.4" stroke-width="1.5"/>
  <text x="540" y="386" text-anchor="middle" fill="#00e676" font-size="12" font-weight="600">Valhalla</text>
  <text x="540" y="404" text-anchor="middle" fill="#56546a" font-size="10">Docker · Walk / Bike / Car</text>
</svg>

## Design Principles

```admonish example title="All In-Memory"
There is no database. All GTFS data is loaded from CSV files at startup and held in memory. This gives extremely fast query times at the cost of startup time (10-30 seconds for index building).
```

```admonish example title="Lock-Free Hot-Reload"
The RAPTOR index is wrapped in [ArcSwap](https://docs.rs/arc-swap), which allows atomic pointer swaps. When new GTFS data is loaded via `POST /api/reload`, the entire index is rebuilt in a background thread and swapped in atomically. No request is ever blocked or sees partial data.
```

```admonish example title="Pattern Grouping"
Trips with identical stop sequences are grouped into **patterns**. This dramatically reduces memory usage and speeds up the RAPTOR scan phase, because the algorithm only needs to evaluate one entry per pattern instead of one per trip.
```

```admonish example title="Indoor Routing"
Valhalla supports indoor maneuvers such as elevators, stairs, escalators, and building enter/exit transitions. Transfers are classified by parent_station: outdoor transfers (different parent_station) always get a Valhalla walking route for the map polyline, while indoor transfers (same parent_station) are only enriched when indoor maneuvers exist in OSM. Transfer polylines use the Valhalla shape (actual walking route) when available, falling back to a straight line otherwise. This enrichment only runs when `maneuvers=true` is requested.
```

```admonish example title="Navitia API Compatibility"
The API mirrors [Navitia](https://navitia.io/) query parameters and response structure, making Glove a potential drop-in replacement for Navitia-based applications.
```

## Technology Stack

| Component | Technology |
|-----------|-----------|
| Backend | Rust, Actix-web 4 |
| Routing | RAPTOR algorithm (custom implementation) |
| Walk/Bike/Car | Valhalla (Docker, with indoor routing support) |
| Frontend | React 19, Vite, MUI 7, Leaflet |
| Data format | GTFS (General Transit Feed Specification) |
| Address search | BAN (Base Adresse Nationale) |
| Serialization | serde (JSON + YAML + CSV) |
| API docs | utoipa (OpenAPI auto-generation) |
| Monitoring | Custom Prometheus metrics |
| Logging | tracing + tracing-subscriber |
