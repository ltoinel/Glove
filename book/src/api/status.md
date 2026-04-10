# Status & Reload

## Status

```
GET /api/status
```

Returns GTFS data statistics and server information. No authentication required.

### Response

```json
{
  "status": "ok",
  "gtfs": {
    "agencies": 42,
    "routes": 1850,
    "stops": 48000,
    "trips": 320000,
    "stop_times": 8500000,
    "calendars": 2500,
    "calendar_dates": 15000,
    "transfers": 95000
  },
  "last_load": "2024-03-15T08:00:00Z"
}
```

This endpoint is used as the Docker healthcheck.

## GTFS Validation

```
GET /api/gtfs/validate
```

Runs 19 automated data quality checks on the loaded GTFS data and returns issues grouped by severity.

### Validation Categories

| Category | Checks |
|----------|--------|
| **Referential Integrity** | Orphaned stop_times (missing trip/stop), orphaned trips (missing route), orphaned routes (missing agency) |
| **Calendar** | Active services exist for today, no expired calendars |
| **Coordinates** | Valid lat/lon ranges, no zeros, within bounds |
| **Transfers** | Valid transfer types, referenced stops exist |
| **Pathways** | Valid pathway types, referenced stops exist |
| **Display** | Missing route colors, missing route names |

### Response

```json
{
  "summary": {
    "errors": 2,
    "warnings": 5,
    "infos": 3,
    "total_checks": 19
  },
  "issues": [
    {
      "severity": "error",
      "category": "referential_integrity",
      "message": "stop_times reference non-existent trip_id",
      "count": 12,
      "samples": ["trip_001", "trip_002", "trip_003"]
    }
  ]
}
```

Each issue includes up to 5 sample IDs for diagnosing the problem. The frontend displays an interactive validation report.

## Reload

```
POST /api/gtfs/reload
```

Triggers a hot-reload of GTFS data. The server remains fully available during the reload.

### Authentication

Requires the API key configured in `config.yaml`:

```bash
curl -X POST http://localhost:8080/api/gtfs/reload \
  -H "Authorization: Bearer your-secret-key"
```

If `api_key` is empty in the config, this endpoint returns `403 Forbidden`.

### How It Works

1. The request is accepted and returns `200 OK` immediately
2. A background thread downloads and parses new GTFS data
3. A fresh RAPTOR index is built from the new data
4. The new index is swapped in atomically via `ArcSwap`
5. In-flight requests continue using the old index until they complete

### Use Cases

- **Scheduled updates**: Call via cron when new GTFS data is published
- **CI/CD**: Trigger after deploying new data files
- **Manual**: Reload after editing GTFS files during development
