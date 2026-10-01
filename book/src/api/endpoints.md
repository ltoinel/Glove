# API Endpoints

Glove exposes a REST API on the configured port (default: 8080). Endpoints return JSON, except `/api/metrics` (Prometheus text) and the tile proxy (PNG images). Errors share one shape:

```json
{ "error": { "id": "bad_request", "message": "'from' must be in 'lon;lat' format" } }
```

## Endpoint Summary

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/journeys/public_transport` | Public transit journey planning (RAPTOR) |
| `GET` | `/api/journeys/walk` | Walking directions (Valhalla) |
| `GET` | `/api/journeys/bike` | Cycling directions (Valhalla, 3 profiles) |
| `GET` | `/api/journeys/car` | Driving directions (Valhalla) |
| `GET` | `/api/places` | Stop and address autocomplete |
| `GET` | `/api/status` | Engine health and map defaults (no GTFS data) |
| `GET` | `/api/gtfs/status` | GTFS data statistics and last load timestamp |
| `GET` | `/api/gtfs/validate` | GTFS data quality validation (39 rules) |
| `POST` | `/api/gtfs/reload` | Hot-reload GTFS data (**API key**) |
| `GET` | `/api/realtime/status` | Real-time transit feed health and schedule-matching counters |
| `GET` | `/api/lines` | Line catalogue for the back-office pickers |
| `GET` | `/api/disruptions` | Disruption catalogue |
| `POST` | `/api/disruptions` | Create a disruption (**API key**) |
| `GET` | `/api/disruptions/active` | Blocking disruptions in force now, resolved to map coordinates |
| `GET` | `/api/disruptions/{id}` | One disruption |
| `PUT` | `/api/disruptions/{id}` | Update a disruption (**API key**) |
| `DELETE` | `/api/disruptions/{id}` | Delete a disruption (**API key**) |
| `GET` | `/api/traffic/geometry` | Road network polylines for the traffic overlay (static, cacheable) |
| `GET` | `/api/traffic/states` | Live road traffic states and events |
| `GET` | `/api/metrics` | Prometheus-format metrics |
| `GET` | `/api/tiles/{z}/{x}/{y}.png` | Map tile proxy with local disk cache |
| `GET` | `/api-docs/openapi.json` | OpenAPI specification |

Disruptions, lines and real-time status are described in [Disruptions & Real-Time](./disruptions.md).

## Query Parameters

All journey endpoints take `from` and `to` as `lon;lat` coordinates; the public transport endpoint also accepts a GTFS `stop_id`. Only the public transport endpoint takes a departure time, `datetime`, in ISO basic format (`YYYYMMDDTHHmmss`, e.g. `20240315T083000`). See [Journey Planning](./journeys.md) for the full parameter lists.

Turn-by-turn maneuvers are **server-controlled** via `routing.maneuvers` in `config.yaml` (disabled by default), not a request parameter. When enabled, walk, bike and car journeys and the `street_network` / `transfer` sections of public transport journeys carry a `maneuvers` array; each maneuver has a `type` field holding the Valhalla maneuver type number, enabling clients to display turn-by-turn navigation with indoor maneuver support. Disabling them reduces response size.

## Authentication

Reads are public. Every write requires the API key configured in `config.yaml` (or the `GLOVE_API_KEY` environment variable):

```yaml
server:
  api_key: "your-secret-key"
```

The protected endpoints are `POST /api/gtfs/reload` and `POST` / `PUT` / `DELETE` on `/api/disruptions`. Pass the key in the `X-Api-Key` header:

```bash
curl -X POST http://localhost:8080/api/gtfs/reload \
  -H "X-Api-Key: your-secret-key"
```

| Situation | Response |
|-----------|----------|
| `server.api_key` empty | `403 Forbidden` (`"id": "disabled"`) — the write endpoints are disabled |
| Header missing or wrong | `401 Unauthorized` (`"id": "unauthorized"`) |

```admonish warning
`GET /api/gtfs/validate` is not protected: it re-reads the GTFS files from disk on every call, which is expensive on a large feed. Restrict it at the reverse proxy if the API is exposed publicly.
```

## Rate Limiting

Every endpoint except the tile proxy and `/api-docs/openapi.json` is rate-limited per client IP address. The limiter is a token bucket: each IP may send a **burst** of `rate_limit` requests, after which tokens refill at **one request per second**. Beyond that the server answers `429 Too Many Requests`.

```yaml
server:
  rate_limit: 20    # burst size per IP; 0 = disabled
```

The built-in default is `20`; the shipped `config.yaml.sample` sets `0` (disabled).

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
