# Installation

## Prerequisites

```admonish info title="Requirements"
- [Rust](https://rustup.rs/) 1.88+
- [Node.js](https://nodejs.org/) 22+ (required by `swagger-client`, a transitive dependency of the API docs viewer)
- [Docker](https://www.docker.com/) for Valhalla walk/bike/car routing — required by `bin/start.sh` (see [Start Valhalla](#start-valhalla)); only a manual `cargo run` works without it
- [Caddy](https://caddyserver.com/docs/install) 2.6+ (HTTPS reverse proxy used by `bin/start.sh`)
- `wget`, `unzip`, `gunzip` (used by `bin/download.sh`) and `curl` (used by `bin/start.sh` to wait for the backend)
- [cargo-watch](https://crates.io/crates/cargo-watch) for `bin/start.sh --dev` (`cargo install cargo-watch`)
```

## Configuration

`config.yaml` is local and git-ignored, since it may hold secrets (API keys). The repository ships `config.yaml.sample`; the `bin/` scripts copy it to `config.yaml` on first run, or do it yourself:

```bash
cp config.yaml.sample config.yaml
```

```admonish warning title="Development API key"
`config.yaml.sample` ships `server.api_key: "glove"`, so a fresh copy enables `POST /api/gtfs/reload` and the disruption writes with a guessable key. Change it, set `GLOVE_API_KEY`, or empty it to disable those endpoints. The Docker API image blanks it (see [Docker](./docker.md)).
```

## Download Data

Glove needs GTFS transit data to operate. The download script reads `config.yaml` for data URLs.

```bash
# Download everything (GTFS + OSM + BAN addresses + traffic geometry)
bin/download.sh all

# Or download individually
bin/download.sh gtfs     # GTFS transit schedules
bin/download.sh osm      # OpenStreetMap data (for Valhalla)
bin/download.sh ban      # BAN French addresses (for autocomplete)
bin/download.sh traffic  # Sytadin road geometry (for the traffic overlay)
```

```admonish note
By default, this downloads data for **Ile-de-France** (Paris region). You can change the data URLs in `config.yaml` to use GTFS feeds from other regions.
```

## Start Valhalla

Valhalla provides walking, cycling, and driving directions, and times the walks to and from stops.

```bash
bin/valhalla.sh start     # Pulls the pinned image, builds tiles, starts on valhalla.port (8002)
bin/valhalla.sh status    # Is the container running?
bin/valhalla.sh stop      # Remove the container (tiles stay in data/valhalla)
```

Without a subcommand the script only prints its usage. `start` creates a Docker container named `glove-valhalla` that builds routing tiles from the OSM data in `data/osm` (run `bin/download.sh osm` first) into `data/valhalla`.

```admonish warning title="Not optional with bin/start.sh"
`bin/start.sh` (production and `--dev` modes) runs `bin/valhalla.sh start` itself when the container is not running, and stops with an error if that fails — no Docker, or no `.pbf` in `data/osm`. To run without Valhalla (public transit only, with straight-line walk estimates), start the backend manually (see [Manual Start](#manual-start)). `--docker` mode brings its own Valhalla service.
```

## Run

`bin/start.sh` puts [Caddy](https://caddyserver.com/) in front of Glove and serves it on two HTTPS domains:

| Domain | Serves |
|--------|--------|
| `https://portal.glove` | The React portal — the static build in production, the Vite dev server (HMR) in dev mode |
| `https://api.glove` | The REST API, reverse-proxied to Actix on `localhost:8080` |

It has three modes, all behind the same two domains:

| Command | Portal | API |
|---------|--------|-----|
| `bin/start.sh` | static build served by Caddy | release binary |
| `bin/start.sh --dev` | Vite dev server (HMR) | `cargo-watch` |
| `bin/start.sh --docker` | `portal` container (nginx) | `api` container |

Everything it starts — Caddy, the backend, Vite, or the Compose stack — stops together with <kbd>Ctrl</kbd>+<kbd>C</kbd>.

### One-time setup

**1. Let Caddy bind ports 80 and 443.** The Debian/Ubuntu package starts its own Caddy service, which already holds those ports; `start.sh` runs its own instance instead:

```bash
sudo systemctl disable --now caddy
sudo setcap cap_net_bind_service=+ep /usr/bin/caddy   # redo after each caddy upgrade
```

**2. Resolve the two names locally.** They are not public domains:

```bash
echo "127.0.0.1 portal.glove api.glove" | sudo tee -a /etc/hosts
```

```admonish tip title="WSL"
A browser running on Windows reads `C:\Windows\System32\drivers\etc\hosts` (edit it as administrator), not the WSL `/etc/hosts`. WSL forwards `localhost` ports to Windows, so `127.0.0.1` works there too.
```

**3. Trust Caddy's local certificate authority.** Certificates are issued by Caddy's own CA (`tls internal`), created on the first start. Until its root is trusted, browsers show a certificate warning:

```bash
caddy trust    # Linux trust store (curl, browsers running on Linux)

# WSL: trust it in the Windows current-user store (no admin rights needed)
certutil.exe -user -addstore Root "$(wslpath -w ~/.local/share/caddy/pki/authorities/local/root.crt)"
```

`start.sh` checks the first two steps and prints what is missing, and prints the trust commands once the CA exists.

### Production Mode

Build and run are separated so restarts are instant:

```bash
bin/build.sh    # Build the release backend binary + portal SPA (run after code changes)
bin/start.sh    # Run only — auto-runs build.sh if artifacts are missing
```

Caddy serves the portal build (`target/portal`) directly: hashed assets are cached for a year, `index.html` is revalidated on every load, and responses are compressed (zstd/gzip). No Node process runs in production.

The portal calls the API on its own domain, so the API origin (`https://api.glove`) is baked into the bundle at build time (`VITE_API_URL`). `start.sh` rebuilds automatically when the artifacts were built for another API domain.

### Development Mode

```bash
bin/start.sh --dev
```

This starts:
- **Backend**: `cargo-watch` for automatic recompilation on Rust file changes
- **Frontend**: Vite dev server on `127.0.0.1:3000`, reached through `https://portal.glove` (HMR runs over the proxied WebSocket)
- **Caddy**: the same two domains as in production

### Docker Mode

```bash
bin/start.sh --docker
```

Builds and runs the `api`, `portal` and `valhalla` images with `docker/docker-compose.yml` (Docker layer caching makes restarts fast), then puts Caddy in front. Rust and Node are not needed on the host. The portal container proxies `/api` to the API container itself, so this mode needs no CORS entry; `https://api.glove` still exposes the API to other clients. See [Docker](./docker.md).

### Custom domains and ports

| Variable | Default | Effect |
|----------|---------|--------|
| `GLOVE_PORTAL_HOST` | `portal.glove` | Portal domain |
| `GLOVE_API_HOST` | `api.glove` | API domain (baked into the portal build) |
| `GLOVE_HTTPS_PORT` / `GLOVE_HTTP_PORT` | `443` / `80` | Caddy ports, e.g. `8443`/`8880` to run without privileged ports. URLs then carry the port (`https://portal.glove:8443`) |

```admonish warning title="CORS"
The browser sends the portal's origin with every API call, and the backend only accepts origins listed in `server.cors_origins` (`config.yaml`; empty by default, `config.yaml.sample` lists `https://portal.glove`). `bin/start.sh` warns when the portal origin is missing (except in `--docker` mode). After changing `GLOVE_PORTAL_HOST` or the HTTPS port, add the new origin there — e.g. `https://portal.glove:8443`.
```

### Manual Start

Without Caddy, the Vite dev server proxies `/api` to the backend on the same origin:

```bash
# Terminal 1 — Backend
cargo run --release

# Terminal 2 — Frontend (http://localhost:3000)
cd portal && npm install && npm run dev
```

## Access

| Service | URL |
|---------|-----|
| Portal (production & dev) | [https://portal.glove](https://portal.glove) |
| API | [https://api.glove/api](https://api.glove/api) |
| OpenAPI spec | [https://api.glove/api-docs/openapi.json](https://api.glove/api-docs/openapi.json) |
| Backend, bypassing Caddy | [http://localhost:8080/api](http://localhost:8080/api) |
