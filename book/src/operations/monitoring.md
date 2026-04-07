# Monitoring

## Health Check

The simplest monitoring is the status endpoint:

```bash
curl http://localhost:8080/api/status
```

This is also used as the Docker healthcheck. A `200 OK` response means the server is running and has GTFS data loaded.

## Prometheus Metrics

Glove exposes metrics at `GET /api/metrics` in Prometheus text format. See the [Metrics](../api/metrics.md) page for details.

## Structured Logging

Glove uses the `tracing` crate for structured logging. Log level is configured in `config.yaml`:

```yaml
server:
  log_level: "info"    # trace, debug, info, warn, error
```

Override at runtime with the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run --release
```

### Log Examples

```
INFO  glove::main > Starting Glove on 0.0.0.0:8080
INFO  glove::gtfs > Loaded 48000 stops, 320000 trips
INFO  glove::raptor > Built RAPTOR index in 12.3s
DEBUG glove::api::journeys > RAPTOR query: 2.3522;48.8566 → 2.2945;48.8584 in 342ms
```

## Rate Limiting

Rate limiting is configured per IP address:

```yaml
server:
  rate_limit: 20    # requests/sec, 0 = disabled
```

When the limit is exceeded, the server returns `429 Too Many Requests`.

## Graceful Shutdown

On `SIGTERM` or `SIGINT`, Glove:
1. Stops accepting new connections
2. Waits up to `shutdown_timeout` seconds for in-flight requests to complete
3. Exits cleanly

```yaml
server:
  shutdown_timeout: 30    # seconds
```
