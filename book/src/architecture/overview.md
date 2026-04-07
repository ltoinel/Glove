# Architecture Overview

Glove is a monorepo with a Rust backend and React frontend.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Frontend (React)                  │
│         Leaflet Map + MUI Sidebar + i18n            │
└───────────────────────┬─────────────────────────────┘
                        │ HTTP/JSON
┌───────────────────────▼─────────────────────────────┐
│                 Actix-web Server                     │
│     CORS · Rate Limiting · Metrics Middleware        │
├─────────────┬──────────────┬───────────┬────────────┤
│  /journeys  │   /places    │ /status   │ /metrics   │
│  /walk      │              │ /reload   │            │
│  /bike /car │              │           │            │
├─────────────┴──────────────┴───────────┴────────────┤
│                                                      │
│   ┌──────────────┐    ┌─────────────┐               │
│   │ RAPTOR Index │    │  BAN Index  │               │
│   │  (ArcSwap)   │    │ (Addresses) │               │
│   └──────┬───────┘    └─────────────┘               │
│          │                                           │
│   ┌──────▼───────┐    ┌─────────────┐               │
│   │  GTFS Data   │    │  Valhalla   │ ◄── Docker    │
│   │  (CSV files) │    │  (External) │               │
│   └──────────────┘    └─────────────┘               │
└──────────────────────────────────────────────────────┘
```

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
Valhalla supports indoor maneuvers such as elevators, stairs, escalators, and building enter/exit transitions. When OSM data includes indoor information, transfer sections in public transport journeys include detailed maneuvers to guide users through stations and buildings. Transfer sections only show maneuvers when indoor data is available from OSM.
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
