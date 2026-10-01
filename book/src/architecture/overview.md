# Architecture Overview

Glove is a monorepo with a Rust backend and React frontend.

## High-Level Architecture

<svg viewBox="0 0 720 530" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Glove high-level architecture" style="max-width:720px;width:100%;font-family:'Inter',-apple-system,BlinkMacSystemFont,sans-serif;">
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
  <text x="200" y="56" text-anchor="middle" fill="#9b9ab2" font-size="10.5">Caddy: portal.glove → api.glove · Docker: nginx /api</text>
  <rect x="380" y="14" width="280" height="54" rx="12" fill="url(#violet)" stroke="#818cf8" stroke-opacity="0.55"/>
  <text x="520" y="38" text-anchor="middle" fill="#a5b4fc" font-size="13" font-weight="700">REST / OpenAPI clients</text>
  <text x="520" y="56" text-anchor="middle" fill="#9b9ab2" font-size="10.5">/api-docs/openapi.json</text>
  <line x1="200" y1="68" x2="200" y2="98" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <line x1="520" y1="68" x2="520" y2="98" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <text x="360" y="88" text-anchor="middle" fill="#62607a" font-size="10">HTTP · JSON · /api/*</text>

  <!-- ===== Actix-web server ===== -->
  <rect x="36" y="100" width="648" height="324" rx="14" fill="rgba(20,20,35,0.55)" stroke="rgba(255,255,255,0.12)"/>
  <text x="360" y="128" text-anchor="middle" fill="#e7e7f0" font-size="14" font-weight="800">Actix-web Server · :8080</text>
  <text x="360" y="146" text-anchor="middle" fill="#9b9ab2" font-size="10.5">CORS · compression · metrics middleware · rate limiting (except tiles + OpenAPI)</text>
  <line x1="56" y1="158" x2="664" y2="158" stroke="rgba(255,255,255,0.10)" stroke-width="1"/>

  <!-- endpoint chips -->
  <g font-size="11" font-weight="700">
    <!-- row 1 -->
    <rect x="56"  y="168" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="129" y="183" text-anchor="middle" fill="#67e8f9">/journeys/*</text>
    <text x="129" y="197" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">public_transport · walk · bike · car</text>
    <rect x="210" y="168" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="283" y="183" text-anchor="middle" fill="#67e8f9">/places</text>
    <text x="283" y="197" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">autocomplete · stops + BAN</text>
    <rect x="364" y="168" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="437" y="183" text-anchor="middle" fill="#67e8f9">/tiles</text>
    <text x="437" y="197" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">cached map proxy</text>
    <rect x="518" y="168" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="591" y="183" text-anchor="middle" fill="#67e8f9">/status</text>
    <text x="591" y="197" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">health · map defaults</text>
    <!-- row 2 -->
    <rect x="56"  y="212" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="129" y="227" text-anchor="middle" fill="#67e8f9">/gtfs/*</text>
    <text x="129" y="241" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">status · validate · reload</text>
    <rect x="210" y="212" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="283" y="227" text-anchor="middle" fill="#67e8f9">/metrics</text>
    <text x="283" y="241" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">Prometheus</text>
    <rect x="364" y="212" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="437" y="227" text-anchor="middle" fill="#67e8f9">/realtime/status</text>
    <text x="437" y="241" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">feed health · match stats</text>
    <rect x="518" y="212" width="146" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="591" y="227" text-anchor="middle" fill="#67e8f9">/lines</text>
    <text x="591" y="241" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">line catalogue</text>
    <!-- row 3 -->
    <rect x="56"  y="256" width="300" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="206" y="271" text-anchor="middle" fill="#67e8f9">/disruptions</text>
    <text x="206" y="285" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">CRUD (X-Api-Key) · /active blockages for the map</text>
    <rect x="364" y="256" width="300" height="36" rx="9" fill="url(#chip)" stroke="#22d3ee" stroke-opacity="0.4"/>
    <text x="514" y="271" text-anchor="middle" fill="#67e8f9">/traffic/*</text>
    <text x="514" y="285" text-anchor="middle" fill="#62607a" font-size="8.5" font-weight="400">geometry (cached 24 h) · live states</text>
  </g>

  <!-- in-memory state, row A -->
  <rect x="56" y="304" width="200" height="50" rx="11" fill="url(#amber)" stroke="#fbbf24" stroke-opacity="0.5"/>
  <text x="156" y="325" text-anchor="middle" fill="#fbbf24" font-size="12.5" font-weight="700">RAPTOR Index</text>
  <text x="156" y="342" text-anchor="middle" fill="#9b9ab2" font-size="9">ArcSwap · lock-free hot-reload</text>
  <rect x="268" y="304" width="192" height="50" rx="11" fill="url(#amber)" stroke="#fbbf24" stroke-opacity="0.35"/>
  <text x="364" y="325" text-anchor="middle" fill="#fbbf24" font-size="12.5" font-weight="700">Real-time overlay</text>
  <text x="364" y="342" text-anchor="middle" fill="#9b9ab2" font-size="9">RealtimeIndex · delays + cancellations</text>
  <rect x="472" y="304" width="192" height="50" rx="11" fill="url(#amber)" stroke="#fbbf24" stroke-opacity="0.35"/>
  <text x="568" y="325" text-anchor="middle" fill="#fbbf24" font-size="12.5" font-weight="700">Disruption catalog</text>
  <text x="568" y="342" text-anchor="middle" fill="#9b9ab2" font-size="9">ArcSwap · authored, persisted as JSON</text>

  <!-- in-memory state, row B -->
  <rect x="56" y="362" width="200" height="50" rx="11" fill="url(#violet)" stroke="#818cf8" stroke-opacity="0.45"/>
  <text x="156" y="383" text-anchor="middle" fill="#a5b4fc" font-size="12.5" font-weight="700">BAN Index</text>
  <text x="156" y="400" text-anchor="middle" fill="#9b9ab2" font-size="9">address geocoding</text>
  <rect x="268" y="362" width="192" height="50" rx="11" fill="url(#violet)" stroke="#818cf8" stroke-opacity="0.45"/>
  <text x="364" y="383" text-anchor="middle" fill="#a5b4fc" font-size="12.5" font-weight="700">Road traffic</text>
  <text x="364" y="400" text-anchor="middle" fill="#9b9ab2" font-size="9">geometry once · states re-published</text>
  <rect x="472" y="362" width="192" height="50" rx="11" fill="rgba(255,255,255,0.035)" stroke="rgba(255,255,255,0.10)"/>
  <text x="568" y="383" text-anchor="middle" fill="#e7e7f0" font-size="12.5" font-weight="600">Tile cache</text>
  <text x="568" y="400" text-anchor="middle" fill="#62607a" font-size="9">data/tiles/ on disk</text>

  <!-- ===== Outside the process ===== -->
  <line x1="129" y1="458" x2="129" y2="428" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <line x1="283" y1="458" x2="283" y2="428" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <line x1="437" y1="458" x2="437" y2="428" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <line x1="591" y1="426" x2="591" y2="456" stroke="#62607a" stroke-width="1.5" marker-end="url(#arr)"/>
  <text x="360" y="446" text-anchor="middle" fill="#62607a" font-size="9.5">loaded at startup · polled · called per request</text>

  <rect x="56" y="460" width="146" height="54" rx="11" fill="rgba(255,255,255,0.035)" stroke="rgba(255,255,255,0.14)"/>
  <text x="129" y="482" text-anchor="middle" fill="#e7e7f0" font-size="11.5" font-weight="600">data/ files</text>
  <text x="129" y="499" text-anchor="middle" fill="#62607a" font-size="8.5">GTFS · BAN · disruptions</text>
  <rect x="210" y="460" width="146" height="54" rx="11" fill="rgba(255,255,255,0.035)" stroke="rgba(255,255,255,0.14)"/>
  <text x="283" y="482" text-anchor="middle" fill="#e7e7f0" font-size="11.5" font-weight="600">GTFS-RT feeds</text>
  <text x="283" y="499" text-anchor="middle" fill="#62607a" font-size="8.5">one poller per feed</text>
  <rect x="364" y="460" width="146" height="54" rx="11" fill="rgba(255,255,255,0.035)" stroke="rgba(255,255,255,0.14)"/>
  <text x="437" y="482" text-anchor="middle" fill="#e7e7f0" font-size="11.5" font-weight="600">Sytadin / DiRIF</text>
  <text x="437" y="499" text-anchor="middle" fill="#62607a" font-size="8.5">MIF/MID + XML states</text>
  <rect x="518" y="460" width="146" height="54" rx="11" fill="url(#green)" stroke="#34d399" stroke-opacity="0.5"/>
  <text x="591" y="482" text-anchor="middle" fill="#34d399" font-size="11.5" font-weight="700">Valhalla · :8002</text>
  <text x="591" y="499" text-anchor="middle" fill="#9b9ab2" font-size="8.5">walk · bike · car · transfers</text>
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
Valhalla supports indoor maneuvers such as elevators, stairs, escalators, and building enter/exit transitions. Every transfer section of a public transport journey is shaped by a Valhalla pedestrian route, which gives the map its polyline and the section its distance; when Valhalla has no answer, a straight line is drawn between the two stops. Transfers are classified by `parent_station`: an *indoor* transfer (both stops in the same station) is routed with zero step and elevator penalties, so the in-station walk is not pushed outside; an *outdoor* transfer uses the regular walker. `routing.maneuvers` in `config.yaml` only decides whether the turn-by-turn maneuvers are attached to the response.
```

```admonish example title="Real-Time as an Overlay"
GTFS-Realtime feeds are polled in the background, one task per feed, and resolved against the loaded schedule into a `RealtimeIndex` of delays and cancellations, published atomically. The router reads the schedule *and* this overlay at query time; the RAPTOR index itself is never rebuilt, since pre-processing takes 10-30 seconds and feeds refresh every 30 seconds by default (`refresh_secs`).
```

```admonish example title="Disruptions Applied at Query Time"
Works, incidents and closures are authored by operators in the back office (`/api/disruptions`, writes guarded by `X-Api-Key`) and persisted as a single JSON document, the only state Glove cannot rebuild from a source file. The disruptions in force at the query instant close stops, remove lines or cut line sections for that search only. When the fastest journey is closed, a second, undisrupted pass recovers it and returns it with `status: "blocked"` and the disruptions explaining why.
```

```admonish example title="Traffic Split by Lifetime"
The road traffic overlay separates what changes from what does not. The Sytadin road geometry is parsed once at startup and served as an immutable body the browser caches for a day; only the segment states are polled and re-published, through an `ArcSwapOption` swapped atomically like the RAPTOR index. Both bodies are serialized once — at startup and at each refresh — never per request, because a snapshot spans several thousand segments.
```

## Technology Stack

| Component | Technology |
|-----------|-----------|
| Backend | Rust, Actix-web 4 |
| Routing | RAPTOR algorithm (custom implementation) |
| Walk/Bike/Car | Valhalla (Docker, with indoor routing support) |
| Frontend | React 19, Vite, MUI 9, Leaflet |
| Data format | GTFS (General Transit Feed Specification) |
| Address search | BAN (Base Adresse Nationale) |
| Road traffic | Sytadin / DiRIF diffusion feed (MIF/MID + XML) |
| Real-time transit | GTFS-Realtime (built-in protobuf wire-format reader) |
| Serialization | serde (JSON + YAML + CSV), bincode for the RAPTOR and BAN caches |
| API docs | utoipa (OpenAPI auto-generation) |
| Monitoring | Custom Prometheus metrics |
| Logging | tracing + tracing-subscriber |
