# Docker

Glove ships **two separate images** — the backend (REST API) and the frontend (portal) — mirroring the two-process design used by `bin/start.sh`. The portal serves the static SPA with nginx and proxies `/api` requests to the backend, so the two stay fully decoupled.

```admonish info title="Same origin, unlike bin/start.sh"
`bin/start.sh` serves the portal and the API on two HTTPS domains through Caddy. The Docker portal image is built **without** `VITE_API_URL`, so the SPA calls `/api` on its own origin and nginx forwards it: no CORS setup is needed. Put your TLS reverse proxy in front of the portal container.
```

| Image | Dockerfile | Serves | Port |
|-------|-----------|--------|------|
| Backend (API) | `docker/Dockerfile.api` | Actix REST API | 8080 |
| Portal | `docker/Dockerfile.portal` | React SPA + `/api` proxy (nginx) | 8080 |

## Run Locally

```bash
bin/start.sh --docker
```

Builds both images, starts the Compose stack (`api`, `portal`, `valhalla`) and serves it through Caddy on `https://portal.glove` and `https://api.glove`, like the other `start.sh` modes. <kbd>Ctrl</kbd>+<kbd>C</kbd> stops Caddy and removes the containers. The one-time setup (ports 80/443, `/etc/hosts`, Caddy's local CA) is described in [Installation](./installation.md#one-time-setup).

## Build the Images

```bash
docker build -f docker/Dockerfile.api    -t glove-api    .
docker build -f docker/Dockerfile.portal -t glove-portal .
```

The backend image builds the Rust binary on `rust:1.98-bookworm` (the crate needs Rust 1.88+, built with `--locked`) and runs it on a minimal `debian:bookworm-slim` runtime. The portal image builds the SPA on `node:25-alpine` and serves it with `nginxinc/nginx-unprivileged:1.31-alpine`. Every base image is pinned by digest; Dependabot bumps tag and digest together.

```admonish note
Both processes are separate. The backend exposes **only** the API on port 8080 — it does not serve any static files. The portal (container port 8080) is what users open in their browser, and it forwards `/api` to the backend.
```

## Run

The portal needs to reach the backend by name, so run both on a shared Docker network (Compose does this for you — see below). Manually:

```bash
docker network create glove-net

docker run -d --name api --network glove-net \
  --user "$(id -u):$(id -g)" --read-only --tmpfs /tmp \
  --cap-drop ALL --security-opt no-new-privileges:true \
  -p 127.0.0.1:8080:8080 \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -e GLOVE_API_KEY="$(openssl rand -hex 24)" \
  glove-api

docker run -d --name portal --network glove-net \
  --read-only --tmpfs /tmp \
  --cap-drop ALL --security-opt no-new-privileges:true \
  -p 127.0.0.1:3000:8080 \
  glove-portal
```

Then open **http://localhost:3000**. The backend:
- Exposes port **8080** (API only)
- Needs the `data/` directory mounted with GTFS data
- Uses the `config.yaml` baked into the image unless one is mounted; the baked copy has an **empty `api_key`**, so reload and disruption writes stay disabled until `GLOVE_API_KEY` is set
- Runs as an unprivileged user (`glove`, uid 10001); `--user` aligns it with the owner of the mounted `data/`, which it writes to (RAPTOR/BAN caches, tiles, disruptions)
- Includes a healthcheck on `GET /api/status`

## Valhalla Container

For walk/bike/car routing, Valhalla runs as a separate container:

```bash
bin/valhalla.sh
```

This script:
1. Pulls the `ghcr.io/gis-ops/docker-valhalla/valhalla` Docker image, pinned by digest (the last upstream build, Valhalla 3.5.1)
2. Builds routing tiles from the downloaded OSM data
3. Starts the container on port **8002**

The Valhalla configuration includes:
- `include_platforms=True` to import platform/indoor data from OSM
- `step_penalty` and `elevator_penalty` in pedestrian costing to fine-tune indoor routing preferences
- Indoor maneuver support (elevator, stairs, escalator, enter/exit building) when OSM data is available

Make sure `config.yaml` points to the Valhalla host:

```yaml
valhalla:
  host: "localhost"    # or the Docker container name if using Docker networking
  port: 8002
```

## Docker Compose

`docker/docker-compose.yml` runs the three services — `api`, `portal` and `valhalla` — on one Compose network. `bin/start.sh --docker` uses it; it also works on its own:

```bash
docker compose -f docker/docker-compose.yml up -d --build
```

The portal's nginx config (`docker/nginx.conf`) proxies `/api` to the `api` service over the Compose network. Open **http://localhost:3000** to use the app.

- **Ports** are published on `127.0.0.1` only (`8080` for the API, `3000` for the portal): Caddy or a local browser reach them, the network does not.
- **Valhalla** uses the same image, options and data (`data/osm`, `data/valhalla`) as `bin/valhalla.sh`, so the routing tiles are shared rather than rebuilt. Its port is not published — the API reaches it over the Compose network — so it does not collide with a `bin/valhalla.sh` container on `8002`.
- **`config.yaml`** is the host's own file, mounted read-only. Only the Valhalla address differs inside Compose, and the `api` service overrides it through the environment:

| Variable | Overrides | Compose value |
|----------|-----------|---------------|
| `GLOVE_VALHALLA_HOST` | `valhalla.host` | `valhalla` |
| `GLOVE_VALHALLA_PORT` | `valhalla.port` | `8002` |
| `GLOVE_API_KEY` | `server.api_key` | passed through from the caller's environment when set |

## Security

The two images Glove builds are hardened; Compose adds runtime restrictions on top:

| Measure | API | Portal |
|---------|-----|--------|
| Runs as non-root | `glove` (Compose: the owner of `data/`, from `GLOVE_UID`/`GLOVE_GID`) | `nginx` (uid 101), master process included |
| Read-only root filesystem | yes (writes go to `data/` and a tmpfs `/tmp`) | yes (tmpfs `/tmp`) |
| Linux capabilities | all dropped | all dropped |
| `no-new-privileges` | yes | yes |
| Secrets in the image | none: `api_key` is blanked at build | none |
| HTTP hardening | — | CSP, `nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy`, `Permissions-Policy` (geolocation allowed for the portal itself), no nginx version |

CI scans both images with [Trivy](https://trivy.dev/) and fails on any HIGH/CRITICAL vulnerability that has a fix; released images carry an SBOM and a SLSA provenance attestation.

```admonish warning title="Valhalla is not hardened"
The third-party Valhalla image calls `sudo` in its entrypoint to fix volume ownership, which `no-new-privileges` and dropped capabilities break. It keeps its defaults, but publishes no port: only the API reaches it, over the Compose network. Upstream has not published a build since 2024-10; the digest pin keeps it from changing silently.
```

```admonish tip title="API key in production"
Do not rely on the key in `config.yaml`: pass a long random `GLOVE_API_KEY` from your secret store. Set but empty, it disables the protected endpoints. The key is compared in constant time.
```

```admonish note title="Published images"
Each GitHub release publishes both images to the GitHub Container Registry: `ghcr.io/ltoinel/glove` (API) and `ghcr.io/ltoinel/glove-portal` (portal).
```
