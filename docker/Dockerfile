# Glove backend (REST API) image.
# The React portal is built and served by a SEPARATE image (docker/Dockerfile.portal),
# which proxies /api requests to this backend. The two processes stay fully decoupled.

# Stage 1: Build backend
FROM rust:1.87-bookworm AS backend
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=backend /app/target/release/glove /app/glove
COPY config.yaml /app/config.yaml

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
  CMD curl -sf http://localhost:8080/api/status || exit 1

ENTRYPOINT ["/app/glove"]
