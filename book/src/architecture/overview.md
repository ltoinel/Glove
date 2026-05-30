# Architecture Overview

Glove is a monorepo with a Rust backend and React frontend.

## High-Level Architecture

<svg viewBox="0 0 720 510" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Glove high-level architecture" style="max-width:720px;width:100%;font-family:'Inter',-apple-system,BlinkMacSystemFont,sans-serif;">
  <defs>
    <linearGradient id="accent" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#22d3ee" stop-opacity="0.20"/><stop offset="100%" stop-color="#818cf8" stop-opacity="0.12"/></linearGradient>
    <linearGradient id="chip" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="#22d3ee" stop-opacity="0.12"/><stop offset="100%" stop-color="#22d3ee" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="amber" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#fbbf24" stop-opacity="0.20"/><stop offset="100%" stop-color="#fbbf24" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="violet" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#818cf8" stop-opacity="0.18"/><stop offset="100%" stop-color="#818cf8" stop-opacity="0.05"/></linearGradient>
    <linearGradient id="green" x1="0" y1="0" x2="1" y2="1"><stop offset="0%" stop-color="#34d399" stop-opacity="0.18"/><stop offset="100%" stop-color="#34d399" stop-opacity="0.05"/></linearGradient>
    <marker id="arr" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="#62607a"/></marker>
  </defs>

  <!-- ===== Clients (two separate processes) ===== -->
  <rect x="60" y="14" width="280" height="54" rx="12" fill="url(#accent)" stroke="#22d3ee" stroke-opacity="0.55"/>
  <text x="200" y="38" text-anchor="middle" fill="#67e8f9" font-size="13" font-weight="700">Portal · React + MUI + Leaflet</text>
  <text x="200" y="56" text-anchor="middle" fill="#9b9ab2" font-size="10.5">Vite dev / nginx · proxies /api → :8080</text>
  <rect x="380" y="14" width="280" height="54" rx="12" fill="url(#violet)" stroke="#818cf8" stroke-opacity="0.55"/>
  <text x="520" y="38" text-anchor="middle" fill="#a5b4fc" font-size="13" font-weight="700">REST / OpenAPI clients</text>
  <text x="520" y="56" text-anchor="middle" fill="#9b9ab2" font-size="10.5">/api-docs/openapi.json</text>
  <line x1="200" y1="68" x2="200" y2="98" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <line x1="520" y1="68" x2="520" y2="98" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <text x="360" y="88" text-anchor="middle" fill="#62607a" font-size="10">HTTP · JSON · /api/*</text>

  <!-- ===== Actix-web server ===== -->
  <rect x="36" y="100" width="648" height="312" rx="14" fill="rgba(20,20,35,0.55)" stroke="rgba(255,255,255,0.12)"/>
  <text x="360" y="128" text-anchor="middle" fill="#e7e7f0" font-size="14" font-weight="800">Actix-web Server · :8080</text>
  <text x="360" y="146" text-anchor="middle" fill="#9b9ab2" font-size="10.5">CORS · rate limiting · metrics middleware</text>
  <line x1="56" y1="158" x2="664" y2="158" stroke="rgba(255,255,255,0.10)" stroke-width="1"/>

  <!-- endpoint chips: row 1 -->
  <g font-size="11.5" font-weight="700">
    <rect x="56"  y="170" width="188" height="40" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="150" y="188" text-anchor="middle" fill="#67e8f9">/journeys/*</text>
    <text x="150" y="202" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">public_transport · walk · bike · car</text>
    <rect x="266" y="170" width="188" height="40" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="360" y="188" text-anchor="middle" fill="#67e8f9">/places</text>
    <text x="360" y="202" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">autocomplete · stops + BAN</text>
    <rect x="476" y="170" width="188" height="40" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="570" y="188" text-anchor="middle" fill="#67e8f9">/tiles</text>
    <text x="570" y="202" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">cached map proxy</text>
    <!-- row 2 -->
    <rect x="56"  y="218" width="188" height="40" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="150" y="236" text-anchor="middle" fill="#67e8f9">/status</text>
    <text x="150" y="250" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">health · map defaults</text>
    <rect x="266" y="218" width="188" height="40" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="360" y="236" text-anchor="middle" fill="#67e8f9">/gtfs/*</text>
    <text x="360" y="250" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">status · validate · reload</text>
    <rect x="476" y="218" width="188" height="40" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="570" y="236" text-anchor="middle" fill="#67e8f9">/metrics</text>
    <text x="570" y="250" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">Prometheus</text>
  </g>

  <!-- in-memory index -->
  <rect x="56" y="272" width="360" height="54" rx="11" fill="url(#amber)" stroke="#fbbf24" stroke-opacity="0.5"/>
  <text x="236" y="296" text-anchor="middle" fill="#fbbf24" font-size="13" font-weight="700">RAPTOR Index</text>
  <text x="236" y="314" text-anchor="middle" fill="#9b9ab2" font-size="9.5">ArcSwap · lock-free hot-reload · ~10k patterns</text>
  <rect x="436" y="272" width="228" height="54" rx="11" fill="url(#violet)" stroke="#818cf8" stroke-opacity="0.45"/>
  <text x="550" y="296" text-anchor="middle" fill="#a5b4fc" font-size="13" font-weight="700">BAN Index</text>
  <text x="550" y="314" text-anchor="middle" fill="#9b9ab2" font-size="9.5">address geocoding</text>

  <!-- sources inside the process -->
  <line x1="236" y1="340" x2="236" y2="328" stroke="#62607a" stroke-width="1.2" marker-end="url(#arr)"/>
  <rect x="56" y="340" width="360" height="54" rx="11" fill="rgba(255,255,255,0.035)" stroke="rgba(255,255,255,0.10)"/>
  <text x="236" y="364" text-anchor="middle" fill="#e7e7f0" font-size="11.5" font-weight="600">GTFS data — CSV, in-memory (no DB)</text>
  <text x="236" y="380" text-anchor="middle" fill="#62607a" font-size="9">stops · trips · stop_times · transfers · pathways</text>
  <rect x="436" y="340" width="228" height="54" rx="11" fill="rgba(255,255,255,0.035)" stroke="rgba(255,255,255,0.10)"/>
  <text x="550" y="364" text-anchor="middle" fill="#e7e7f0" font-size="11.5" font-weight="600">Tile cache</text>
  <text x="550" y="380" text-anchor="middle" fill="#62607a" font-size="9">data/tiles/ on disk</text>

  <!-- ===== Valhalla (separate container) ===== -->
  <line x1="360" y1="412" x2="360" y2="438" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <text x="436" y="430" text-anchor="middle" fill="#62607a" font-size="9.5">routing + indoor transfers</text>
  <rect x="190" y="440" width="340" height="56" rx="12" fill="url(#green)" stroke="#34d399" stroke-opacity="0.5"/>
  <text x="360" y="465" text-anchor="middle" fill="#34d399" font-size="13" font-weight="700">Valhalla · Docker · :8002</text>
  <text x="360" y="483" text-anchor="middle" fill="#9b9ab2" font-size="9.5">walk · bike · car · indoor transfer enrichment</text>
</svg>

## Design Principles

```admonish example title="All In-Memory"
There is no database. All GTFS data is loaded from CSV files at startup and held in memory. This gives extremely fast query times at the cost of startup time (10-30 seconds for index building).
```

```admonish example title="Lock-Free Hot-Reload"
The RAPTOR index is wrapped in [ArcSwap](https://docs.rs/arc-swap), which allows atomic pointer swaps. When new GTFS data is loaded via `POST /api/gtfs/reload`, the entire index is rebuilt in a background thread and swapped in atomically. No request is ever blocked or sees partial data.
```

```admonish example title="Pattern Grouping"
Trips with identical stop sequences are grouped into **patterns**. This dramatically reduces memory usage and speeds up the RAPTOR scan phase, because the algorithm only needs to evaluate one entry per pattern instead of one per trip.
```

```admonish example title="Indoor Routing"
Valhalla supports indoor maneuvers such as elevators, stairs, escalators, and building enter/exit transitions. Transfers are classified by parent_station: outdoor transfers (different parent_station) always get a Valhalla walking route for the map polyline, while indoor transfers (same parent_station) are only enriched when indoor maneuvers exist in OSM. Transfer polylines use the Valhalla shape (actual walking route) when available, falling back to a straight line otherwise. This enrichment only runs when `routing.maneuvers` is enabled in `config.yaml`.
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
