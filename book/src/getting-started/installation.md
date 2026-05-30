# Installation

## Prerequisites

```admonish info title="Requirements"
- [Rust](https://rustup.rs/) 1.85+
- [Node.js](https://nodejs.org/) 18+
- [Docker](https://www.docker.com/) (optional, for Valhalla walk/bike/car routing)
```

## Download Data

Glove needs GTFS transit data to operate. The download script reads `config.yaml` for data URLs.

```bash
# Download everything (GTFS + OSM + BAN addresses)
bin/download.sh all

# Or download individually
bin/download.sh gtfs     # GTFS transit schedules
bin/download.sh osm      # OpenStreetMap data (for Valhalla)
bin/download.sh ban      # BAN French addresses (for autocomplete)
```

```admonish note
By default, this downloads data for **Ile-de-France** (Paris region). You can change the data URLs in `config.yaml` to use GTFS feeds from other regions.
```

## Start Valhalla (Optional)

Valhalla provides walking, cycling, and driving directions. Without it, only public transit routing is available.

```bash
bin/valhalla.sh    # Pulls Docker image, builds tiles, starts on port 8002
```

This creates a Docker container named `valhalla` that builds routing tiles from the downloaded OSM data.

## Run

### Production Mode

Build and run are separated so restarts are instant:

```bash
bin/build.sh    # Build the release backend binary + portal SPA (run after code changes)
bin/start.sh    # Run only — auto-runs build.sh on the first launch if artifacts are missing
```

`start.sh` launches **two separate processes**: the Actix API on port **8080** and the frontend (`vite preview`) on port **3000**. The frontend proxies `/api` requests to the backend, so you access the app at **http://localhost:3000**.

```admonish note
`vite preview` is fine for local/self-hosted use, but it is not a hardened production web server. For real production, use the [Docker](./docker.md) setup (nginx-served portal) behind a TLS reverse proxy.
```

### Development Mode

```bash
bin/start.sh --dev
```

This starts:
- **Backend**: `cargo-watch` for automatic recompilation on Rust file changes
- **Frontend**: Vite dev server with HMR on port **3000**

### Manual Start

```bash
# Terminal 1 — Backend
cargo run --release

# Terminal 2 — Frontend
cd portal && npm install && npm run dev
```

## Access

| Service | URL |
|---------|-----|
| Portal (production & dev) | [http://localhost:3000](http://localhost:3000) |
| API | [http://localhost:8080/api](http://localhost:8080/api) |
| OpenAPI spec | [http://localhost:8080/api-docs/openapi.json](http://localhost:8080/api-docs/openapi.json) |
