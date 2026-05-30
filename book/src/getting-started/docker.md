# Docker

Glove ships **two separate images** — the backend (REST API) and the frontend (portal) — mirroring the two-process design used by `bin/start.sh`. The portal serves the static SPA with nginx and proxies `/api` requests to the backend, so the two stay fully decoupled.

| Image | Dockerfile | Serves | Port |
|-------|-----------|--------|------|
| Backend (API) | `docker/Dockerfile` | Actix REST API | 8080 |
| Portal | `docker/Dockerfile.portal` | React SPA + `/api` proxy (nginx) | 80 |

## Build the Images

```bash
docker build -f docker/Dockerfile        -t glove-api    .
docker build -f docker/Dockerfile.portal -t glove-portal .
```

The backend image builds the Rust binary on `rust:1.87` and runs it on a minimal `debian:bookworm-slim` runtime. The portal image builds the SPA on `node:20-alpine` and serves it with `nginx:1.27-alpine`.

```admonish note
Both processes are separate. The backend exposes **only** the API on port 8080 — it does not serve any static files. The portal (port 80) is what users open in their browser, and it forwards `/api` to the backend.
```

## Run

The portal needs to reach the backend by name, so run both on a shared Docker network (Compose does this for you — see below). Manually:

```bash
docker network create glove-net

docker run -d --name api --network glove-net \
  -p 8080:8080 \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/config.yaml:/app/config.yaml \
  glove-api

docker run -d --name portal --network glove-net \
  -p 3000:80 \
  glove-portal
```

Then open **http://localhost:3000**. The backend:
- Exposes port **8080** (API only)
- Needs the `data/` directory mounted with GTFS data
- Needs `config.yaml` mounted for configuration
- Includes a healthcheck on `GET /api/status`

## Valhalla Container

For walk/bike/car routing, Valhalla runs as a separate container:

```bash
bin/valhalla.sh
```

This script:
1. Pulls the `ghcr.io/valhalla/valhalla` Docker image
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

## Docker Compose (Example)

```yaml
services:
  api:
    build:
      context: .
      dockerfile: docker/Dockerfile
    ports:
      - "8080:8080"
    volumes:
      - ./data:/app/data
      - ./config.yaml:/app/config.yaml
    depends_on:
      - valhalla

  portal:
    build:
      context: .
      dockerfile: docker/Dockerfile.portal
    ports:
      - "3000:80"
    depends_on:
      - api

  valhalla:
    image: ghcr.io/valhalla/valhalla:latest
    ports:
      - "8002:8002"
    volumes:
      - ./data/osm:/custom_files
```

The portal's nginx config (`docker/nginx.conf`) proxies `/api` to the `api` service over the Compose network. Open **http://localhost:3000** to use the app.

```admonish warning
Adjust `valhalla.host` in `config.yaml` to `valhalla` (the service name) when using Docker Compose networking.
```
