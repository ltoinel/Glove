# Docker

Glove provides a multi-stage Dockerfile for containerized deployment.

## Build the Image

```bash
docker build -f docker/Dockerfile -t glove .
```

The build uses three stages:

1. **Node.js** (node:20-alpine) — builds the React frontend with Vite
2. **Rust** (rust:1.87) — compiles the backend in release mode
3. **Runtime** (debian:bookworm-slim) — minimal image with just the binary and static files

## Run

```bash
docker run -d \
  --name glove \
  -p 8080:8080 \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/config.yaml:/app/config.yaml \
  glove
```

The container:
- Exposes port **8080**
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
version: "3.8"
services:
  glove:
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

  valhalla:
    image: ghcr.io/valhalla/valhalla:latest
    ports:
      - "8002:8002"
    volumes:
      - ./data/osm:/custom_files
```

```admonish warning
Adjust `valhalla.host` in `config.yaml` to `valhalla` (the service name) when using Docker Compose networking.
```
