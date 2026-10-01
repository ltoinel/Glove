# Monitoring

## Health Check

The simplest monitoring is the status endpoint:

```bash
curl http://localhost:8080/api/status
```

This is also used as the API image's Docker healthcheck. A `200 OK` response means the server is running — it only starts listening once the GTFS data is loaded. The body reports engine health (dependencies such as Valhalla) and map defaults, not GTFS data.

Two more endpoints help when something looks off:

| Endpoint | Shows |
|----------|-------|
| `GET /api/gtfs/status` | GTFS record counts, RAPTOR pattern/service counts and the last load timestamp (`loaded_at`) — confirms a hot-reload took effect |
| `GET /api/realtime/status` | Per-feed health of the real-time transit feeds, with schedule-matching counters that reveal a feed answering 200 but matching nothing |

## Prometheus Metrics

Glove exposes metrics at `GET /api/metrics` in Prometheus text format. See the [Metrics](../api/metrics.md) page for details.

## Structured Logging

Glove uses the `tracing` crate for structured logging. Log level is configured in `config.yaml`:

```yaml
server:
  log_level: "warn"    # trace, debug, info, warn, error
```

`config.yaml.sample` ships with `warn`, which keeps the startup progress messages below out of the log; the built-in default when the key is absent is `info`.

Override at runtime with the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug cargo run --release
```

### Log Examples

At `info`, a cold start (no RAPTOR cache) logs lines such as:

```
2026-09-28T20:57:41.120Z  INFO glove::transit::gtfs: 496393 trips
2026-09-28T20:57:52.874Z  INFO glove::transit::gtfs: 11019607 stop_times
2026-09-28T20:57:53.301Z  INFO glove::transit::raptor: Building RAPTOR index...
2026-09-28T20:58:09.962Z  INFO glove::transit::raptor: RAPTOR index built
2026-09-28T20:58:10.534Z  INFO glove: 10001 patterns, 53446 stops
2026-09-28T20:58:10.540Z  INFO glove: Starting server on http://0.0.0.0:8080
```

When the cached index matches the GTFS fingerprint, the build lines are replaced by `RAPTOR index loaded from cache (…)`. Individual journey queries are not logged.

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
