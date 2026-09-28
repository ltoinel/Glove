# Installation

## Prerequisites

```admonish info title="Requirements"
- [Rust](https://rustup.rs/) 1.88+
- [Node.js](https://nodejs.org/) 22+ (required by `swagger-client`, a transitive dependency of the API docs viewer)
- [Docker](https://www.docker.com/) (optional, for Valhalla walk/bike/car routing)
- [Caddy](https://caddyserver.com/docs/install) 2.6+ (HTTPS reverse proxy used by `bin/start.sh`)
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

## Start Valhalla (Optional)

Valhalla provides walking, cycling, and driving directions. Without it, only public transit routing is available.

```bash
bin/valhalla.sh    # Pulls Docker image, builds tiles, starts on port 8002
```

This creates a Docker container named `valhalla` that builds routing tiles from the downloaded OSM data.

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
The browser sends the portal's origin with every API call, and the backend only accepts origins listed in `server.cors_origins` (`config.yaml`, default `https://portal.glove`). After changing `GLOVE_PORTAL_HOST` or the HTTPS port, add the new origin there — e.g. `https://portal.glove:8443`.
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
