# Configuration

Glove is configured via `config.yaml` at the repository root. All settings have sensible defaults.

## Server

```yaml
server:
  bind: "0.0.0.0"
  port: 8080
  workers: 1                    # 0 = auto (one per logical CPU)
  log_level: "info"             # trace, debug, info, warn, error
  shutdown_timeout: 30          # seconds — graceful shutdown for in-flight requests
  api_key: ""                   # Required for POST /api/reload. Empty = endpoint disabled
  cors_origins: []              # Allowed origins. ["*"] = permissive (not for production)
  rate_limit: 20                # Max requests/sec per IP. 0 = disabled
```

| Setting | Description | Default |
|---------|-------------|---------|
| `bind` | Network interface to listen on | `0.0.0.0` |
| `port` | HTTP port | `8080` |
| `workers` | Actix worker threads. `0` = one per CPU core | `1` |
| `log_level` | Minimum log level | `info` |
| `shutdown_timeout` | Seconds to wait for in-flight requests on shutdown | `30` |
| `api_key` | API key for the reload endpoint. Empty disables the endpoint | `""` |
| `cors_origins` | List of allowed CORS origins. `["*"]` allows all | `[]` |
| `rate_limit` | Maximum requests per second per IP address | `20` |

```admonish tip
Override the log level at runtime with `RUST_LOG=debug cargo run`.
```

## Data Sources

```yaml
data:
  dir: "data"
  gtfs_url: "https://data.iledefrance-mobilites.fr/..."
  osm_url: "https://download.geofabrik.de/europe/france/ile-de-france-latest.osm.pbf"
  ban_url: "https://adresse.data.gouv.fr/data/ban/adresses/latest/csv"
  departments: [75, 77, 78, 91, 92, 93, 94, 95]
```

| Setting | Description |
|---------|-------------|
| `dir` | Base data directory. Sub-directories `gtfs/`, `osm/`, `raptor/`, `ban/` are created automatically |
| `gtfs_url` | URL to download the GTFS zip archive |
| `osm_url` | URL to download the OpenStreetMap PBF file (for Valhalla) |
| `ban_url` | Base URL for BAN address CSV files |
| `departments` | French department codes to download BAN data for |

## Routing

```yaml
routing:
  max_journeys: 5
  max_transfers: 5
  default_transfer_time: 120    # seconds
  max_duration: 10800           # 3 hours in seconds
  max_nearest_stop_distance: 1500  # meters (~20 min walk at 5 km/h)
```

| Setting | Description | Default |
|---------|-------------|---------|
| `max_journeys` | Maximum number of alternative journeys to return | `5` |
| `max_transfers` | Maximum number of transfers in a journey | `5` |
| `default_transfer_time` | Default walking time between stops (seconds) | `120` |
| `max_duration` | Maximum total journey duration (seconds) | `10800` (3h) |
| `max_nearest_stop_distance` | Maximum distance to nearest stops (meters) | `1500` |

## Valhalla

```yaml
valhalla:
  host: "localhost"
  port: 8002
```

The Valhalla routing engine is used for walking, cycling, and driving directions. It runs as a separate Docker container.

## Map

```yaml
map:
  zoom: 11
  center_lat: 48.8566
  center_lon: 2.3522
  bounds_sw_lat: 48.1
  bounds_sw_lon: 1.4
  bounds_ne_lat: 49.3
  bounds_ne_lon: 3.6
```

These settings control the initial map view and geographic bounds in the frontend.

## Bike Profiles

```yaml
bike:
  city:
    cycling_speed: 16.0         # km/h
    use_roads: 0.2              # prefer bike lanes
    use_hills: 0.3              # avoid climbs
    bicycle_type: "City"
  ebike:
    cycling_speed: 21.0
    use_roads: 0.4
    use_hills: 0.8              # climbs are easy with motor
    bicycle_type: "Hybrid"
  road:
    cycling_speed: 25.0
    use_roads: 0.6
    use_hills: 0.5
    bicycle_type: "Road"
```

Three bike profiles are available, each with independent Valhalla routing parameters:

| Profile | Speed | Use Case |
|---------|-------|----------|
| **City** | 16 km/h | Velib' / city bikes, avoids hills and busy roads |
| **E-bike** | 21 km/h | Electric bikes (VAE), handles hills easily |
| **Road** | 25 km/h | Road bikes, prefers smooth tarmac |
