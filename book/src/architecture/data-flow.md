# Data Flow

## Startup Sequence

```
config.yaml
    │
    ▼
┌─────────────┐     ┌──────────────┐
│ Load Config │────►│ Check Cache  │
└─────────────┘     └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              │ Cache valid?            │
              ▼                         ▼
    ┌─────────────────┐     ┌───────────────────┐
    │ Load from disk  │     │ Parse GTFS CSVs   │
    │ (sub-second)    │     │ Build RAPTOR index │
    └────────┬────────┘     │ (10-30 seconds)   │
             │              │ Save to cache      │
             │              └────────┬──────────┘
             │                       │
             └───────────┬───────────┘
                         ▼
              ┌──────────────────┐
              │ Load BAN data    │
              │ (addresses)      │
              └────────┬─────────┘
                       ▼
              ┌──────────────────┐
              │ Start Actix-web  │
              │ Serve API + SPA  │
              └──────────────────┘
```

## GTFS Data Model

Glove loads the following GTFS files:

| File | Content | Rust Struct |
|------|---------|-------------|
| `agency.txt` | Transit agencies | `Agency` |
| `routes.txt` | Transit routes (lines) | `Route` |
| `stops.txt` | Stop locations | `Stop` |
| `trips.txt` | Individual trips | `Trip` |
| `stop_times.txt` | Arrival/departure at each stop | `StopTime` |
| `calendar.txt` | Weekly service schedules | `Calendar` |
| `calendar_dates.txt` | Service exceptions | `CalendarDate` |
| `transfers.txt` | Transfer connections between stops | `Transfer` |

## Query Flow

### Public Transit Journey

```
Client Request
    │
    ▼
┌────────────────────────────┐
│ Parse query parameters     │
│ (from, to, datetime, etc.) │
└────────────┬───────────────┘
             ▼
┌────────────────────────────┐
│ Find nearest stops to      │
│ origin and destination     │
└────────────┬───────────────┘
             ▼
┌────────────────────────────┐
│ Run RAPTOR (round 1)       │◄──┐
│ Collect Pareto-optimal     │   │
│ journeys                   │   │
└────────────┬───────────────┘   │
             ▼                   │
┌────────────────────────────┐   │
│ Enough journeys?           │   │
│ No → Exclude used patterns │───┘
│ Yes → Continue             │
└────────────┬───────────────┘
             ▼
┌────────────────────────────┐
│ Reconstruct journeys       │
│ Tag: fastest, least walks  │
│ Format Navitia response    │
└────────────┬───────────────┘
             ▼
         JSON Response
```

### Walk / Bike / Car Journey

```
Client Request
    │
    ▼
┌────────────────────────────┐
│ Build Valhalla request     │
│ (costing model + options)  │
└────────────┬───────────────┘
             ▼
┌────────────────────────────┐
│ Call Valhalla /route API   │
│ (HTTP on localhost:8002)   │
└────────────┬───────────────┘
             ▼
┌────────────────────────────┐
│ Decode polyline            │
│ Extract maneuvers          │
│ Compute elevation colors   │ (bike only)
│ Format Navitia response    │
└────────────┬───────────────┘
             ▼
         JSON Response
```

## Hot Reload

The hot reload mechanism allows updating GTFS data without downtime:

1. `POST /api/reload` is called (requires `api_key`)
2. A background thread loads new GTFS data and builds a fresh RAPTOR index
3. The new index is swapped in atomically via `ArcSwap`
4. All in-flight requests continue using the old index until they complete
5. The old index is dropped when the last reference is released
