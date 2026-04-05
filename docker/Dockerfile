# Stage 1: Build frontend
FROM node:20-alpine AS frontend
WORKDIR /app/portal
COPY portal/package.json portal/package-lock.json* ./
RUN npm ci --silent
COPY portal/ ./
RUN npx vite build --outDir /app/portal-dist --emptyOutDir

# Stage 2: Build backend
FROM rust:1.87-bookworm AS backend
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=backend /app/target/release/glove /app/glove
COPY --from=frontend /app/portal-dist /app/target/portal
COPY config.yaml /app/config.yaml

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
  CMD curl -sf http://localhost:8080/api/status || exit 1

ENTRYPOINT ["/app/glove"]
