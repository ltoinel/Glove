# API Endpoints

Glove exposes a REST API on the configured port (default: 8080). All endpoints return JSON.

## Endpoint Summary

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/journeys/public_transport` | Public transit journey planning (RAPTOR) |
| `GET` | `/api/journeys/walk` | Walking directions (Valhalla) |
| `GET` | `/api/journeys/bike` | Cycling directions (Valhalla, 3 profiles) |
| `GET` | `/api/journeys/car` | Driving directions (Valhalla) |
| `GET` | `/api/places` | Stop and address autocomplete |
| `GET` | `/api/status` | GTFS stats and server status |
| `POST` | `/api/reload` | Hot-reload GTFS data |
| `GET` | `/api/metrics` | Prometheus-format metrics |
| `GET` | `/api-docs/openapi.json` | OpenAPI specification |

## Navitia Compatibility

The journey planning endpoints use query parameters compatible with the [Navitia](https://navitia.io/) API:

- `from` — Origin coordinates (`lon;lat`)
- `to` — Destination coordinates (`lon;lat`)
- `datetime` — Departure time (ISO 8601)

This allows Glove to serve as a drop-in replacement for Navitia in existing applications.

All journey endpoints accept an optional `maneuvers=true` query parameter. When enabled, responses include a `maneuver_type` field (Valhalla type number) in maneuver objects, enabling clients to display turn-by-turn navigation with indoor maneuver support. Maneuvers are disabled by default to reduce response size and skip transfer Valhalla enrichment.

## Authentication

Most endpoints are public. The `POST /api/reload` endpoint requires an API key configured in `config.yaml`:

```yaml
server:
  api_key: "your-secret-key"
```

Pass the key in the `Authorization` header:

```bash
curl -X POST http://localhost:8080/api/reload \
  -H "Authorization: Bearer your-secret-key"
```

```admonish warning
If `api_key` is empty in the config, the reload endpoint is disabled.
```

## Rate Limiting

All endpoints are rate-limited per IP address. The default is 20 requests/second, configurable via:

```yaml
server:
  rate_limit: 20    # 0 = disabled
```

## CORS

CORS is configured via `config.yaml`:

```yaml
server:
  cors_origins: []              # Default: restrictive
  cors_origins: ["*"]           # Permissive (not for production)
  cors_origins: ["https://example.com"]  # Specific origins
```

## OpenAPI Documentation

The full API specification is auto-generated and available at:

```
GET /api-docs/openapi.json
```

The frontend includes a Swagger UI viewer for interactive API exploration.
