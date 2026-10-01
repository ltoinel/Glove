# Status & Reload

## Status

```
GET /api/status
```

Returns engine **health** and **map defaults** only — no GTFS data (those moved
to `GET /api/gtfs/status`). No authentication required. Used as the Docker healthcheck.

### Response

```json
{
  "status": "ok",
  "dependencies": {
    "valhalla": "ok"
  },
  "map": {
    "center": [48.8566, 2.3522],
    "zoom": 11,
    "bounds": [[48.1, 1.4], [49.3, 3.6]]
  }
}
```

`status` is `"ok"` when all dependencies are healthy, `"degraded"` otherwise; `dependencies.valhalla` is then `"unreachable"`. The endpoint always answers `200`.

## GTFS Status

```
GET /api/gtfs/status
```

Returns GTFS data statistics and the last load timestamp.

### Response

```json
{
  "loaded_at": "2026-09-28T20:58:10.534972235+00:00",
  "gtfs": {
    "agencies": 62,
    "routes": 2025,
    "stops": 53446,
    "trips": 496393,
    "stop_times": 11019607,
    "calendars": 1010,
    "calendar_dates": 2140,
    "transfers": 192366
  },
  "raptor": {
    "patterns": 10001,
    "services": 1046
  }
}
```

`loaded_at` is the RFC 3339 time (UTC) the served index was built.

## GTFS Validation

```
GET /api/gtfs/validate
```

Runs a catalogue of **39 data quality rules** and returns every rule's outcome plus the issues found. The feed is re-read **from the GTFS files on disk**, not from the served index, so the report reflects a new download before it is hot-reloaded. No authentication required; on a large feed a call takes as long as a GTFS load.

### Rules by Category

| Category | Rules |
|----------|-------|
| `loading` | Every row can be parsed; IDs are unique within their file |
| `referential_integrity` | stop_times reference existing trips and stops; trips reference existing routes and defined services; routes reference existing agencies; `parent_station` references existing stops; stop_times serve platforms, not stations or entrances; station hierarchy follows `location_type`; every platform is served by a trip; every route has trips; `route_type` is a known GTFS mode |
| `calendar` | Calendar periods start before they end; the feed is valid today and for the coming days; services run today; every service runs at least one day; every service is used by a trip |
| `schedule` | Scheduled times are valid `HH:MM:SS`; on-demand (GTFS-Flex) calls; `stop_sequence` is unique within a trip; times never go backwards within a trip; every trip has stop_times; every trip serves at least two timed stops; speeds between consecutive stops are plausible for the mode; boarding and alighting restrictions (`pickup_type` / `drop_off_type`) |
| `coordinates` | Stops have valid coordinates; stops lie inside the configured map area (`map.bounds_*`) |
| `transfers` | Same-name stops are grouped under a station; transfers reference existing stops; `min_transfer_time` is plausible; transfers can be walked in their `min_transfer_time`; stops of a station are connected to each other |
| `pathways` | Pathways reference existing stops; pathways have a `traversal_time`; `pathway_mode` is a known value (1-7) |
| `display` | Stops have a name; route colors are valid hex; trips have a headsign |

Rules that need `location_type` find nothing (and report `passed`) when that column cannot be loaded.

### Response

```json
{
  "generated_at": "2026-10-02T01:15:42.120394+02:00",
  "reference_date": "2026-10-02",
  "feed": {
    "agencies": 62,
    "routes": 2025,
    "stops": 53446,
    "trips": 496393,
    "stop_times": 11019607,
    "calendars": 1010,
    "calendar_dates": 2140,
    "transfers": 192366,
    "pathways": 0,
    "service_start": "2026-09-28",
    "service_end": "2026-11-30"
  },
  "summary": {
    "errors": 1,
    "warnings": 4,
    "infos": 2,
    "total_checks": 39
  },
  "rules": [
    {
      "id": "malformed_rows",
      "title": "Every row can be parsed",
      "category": "loading",
      "status": "passed"
    },
    {
      "id": "time_travel",
      "title": "Times never go backwards within a trip",
      "category": "schedule",
      "status": "error"
    }
  ],
  "issues": [
    {
      "rule": "time_travel",
      "severity": "error",
      "category": "schedule",
      "message": "Stop times going back in time within a trip",
      "count": 12,
      "samples": ["trip=T1 seq=4: departs 08:10:00 before arriving 08:12:00"]
    }
  ]
}
```

| Field | Description |
|-------|-------------|
| `generated_at` | When the validation ran (RFC 3339, local offset) |
| `reference_date` | The day the "today" calendar rules were evaluated against |
| `feed` | Size of the validated feed; `service_start` / `service_end` (`YYYY-MM-DD`) are `null` when no service is defined |
| `summary` | Issue counts per severity; `total_checks` is the number of rules run |
| `rules` | Every rule in catalogue order; `status` is `passed`, or the worst severity it raised (`error`, `warning`, `info`) |
| `issues` | Findings sorted by severity (errors first), each with the `rule` that raised it, the number of affected entities in `count`, and up to 20 samples in `samples` (affected IDs or short descriptions; omitted when empty) |

The frontend displays an interactive validation report. If the files cannot be loaded, the endpoint returns `500` with `"id": "load_failed"`.

## Reload

```
POST /api/gtfs/reload
```

Rebuilds the RAPTOR index from the GTFS files on disk and swaps it in. The server keeps answering queries during the reload.

### Authentication

Requires the API key configured in `server.api_key` (or `GLOVE_API_KEY`), passed in the `X-Api-Key` header:

```bash
curl -X POST http://localhost:8080/api/gtfs/reload \
  -H "X-Api-Key: your-secret-key"
```

If `api_key` is empty in the config, this endpoint returns `403 Forbidden`; a missing or wrong key returns `401 Unauthorized`.

### How It Works

1. The GTFS files under `{data.dir}` are parsed on a blocking thread pool. Nothing is downloaded: fetch new data first (e.g. `bin/download.sh`)
2. A fresh RAPTOR index is built from them, and saved to the RAPTOR cache for the next startup
3. The new index is swapped in atomically via `ArcSwap`
4. In-flight requests continue using the old index until they complete

The request is **synchronous**: it answers once the new index is live, which takes as long as a startup build (tens of seconds on a large feed).

### Response

`200 OK` with the statistics of the new index, the same payload as `GET /api/gtfs/status` plus a `status` field:

```json
{
  "status": "reloaded",
  "loaded_at": "2026-10-02T06:00:12.481230117+00:00",
  "gtfs": { "agencies": 62, "routes": 2025, "stops": 53446, ... },
  "raptor": { "patterns": 10001, "services": 1046 }
}
```

On failure the old index keeps serving and the endpoint returns `500` with `"id": "reload_failed"` (the files could not be loaded) or `"reload_panic"`.

### Use Cases

- **Scheduled updates**: Call via cron after downloading new GTFS data
- **CI/CD**: Trigger after deploying new data files
- **Manual**: Reload after editing GTFS files during development
