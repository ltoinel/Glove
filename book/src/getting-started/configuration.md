# Configuration

Glove is configured via `config.yaml` at the repository root. All settings have sensible defaults.

The file is git-ignored: start from `config.yaml.sample` (the `bin/` scripts copy it on first run). Without a `config.yaml`, the backend runs on built-in defaults.

```admonish note title="Defaults vs. config.yaml.sample"
The **Default** columns below give the built-in value, used when a key is absent. `config.yaml.sample` — and so a fresh `config.yaml` — sets several keys differently:

| Key | Built-in default | `config.yaml.sample` |
|-----|------------------|----------------------|
| `server.log_level` | `info` | `warn` |
| `server.api_key` | `""` (disabled) | `"glove"` — change it, see below |
| `server.cors_origins` | `[]` | `["https://portal.glove"]` |
| `server.rate_limit` | `20` | `0` (disabled) |
| `routing.max_transfers` | `5` | `4` |
| `routing.prefer_rail` | `false` | `true` |
| `traffic.enabled` | `false` | `true` |
| `traffic.refresh_secs` | `60` | `600` |
| `map.tile_cache_duration` | `86400` (1 day) | `864000` (10 days) |
```

## Server

```yaml
server:
  bind: "0.0.0.0"
  port: 8080
  workers: 0                    # 0 = auto (one per logical CPU)
  log_level: "info"             # trace, debug, info, warn, error
  shutdown_timeout: 30          # seconds — graceful shutdown for in-flight requests
  api_key: ""                   # Required for POST /api/gtfs/reload and disruption writes. Empty = disabled
  cors_origins:                 # Allowed origins. ["*"] = permissive (not for production)
    - "https://portal.glove"    # portal served by Caddy (bin/start.sh)
  rate_limit: 20                # Max requests/sec per IP. 0 = disabled
```

| Setting | Description | Default |
|---------|-------------|---------|
| `bind` | Network interface to listen on | `0.0.0.0` |
| `port` | HTTP port | `8080` |
| `workers` | Actix worker threads. `0` = one per CPU core | `0` |
| `log_level` | Minimum log level | `info` |
| `shutdown_timeout` | Seconds to wait for in-flight requests on shutdown | `30` |
| `api_key` | API key (`X-Api-Key`) for `POST /api/gtfs/reload` and disruption writes. Empty disables them. Overridden by `GLOVE_API_KEY` | `""` |
| `cors_origins` | List of allowed CORS origins. `["*"]` allows all; empty rejects every cross-origin call. `bin/start.sh` serves the portal on its own domain, so its origin must be listed (see [Custom domains and ports](./installation.md#custom-domains-and-ports)) | `[]` (`config.yaml.sample` lists `https://portal.glove`) |
| `rate_limit` | Maximum requests per second per IP address. `0` disables limiting | `20` |

```admonish warning title="Development API key"
`config.yaml.sample` ships `api_key: "glove"`, so every install created from it has the reload and disruption-write endpoints enabled with a guessable key. Replace it with a long random value (or set `GLOVE_API_KEY`), or set it to `""` to disable those endpoints.
```

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
| `dir` | Base data directory. Sub-directories are implicit: `gtfs/`, `osm/`, `raptor/` (index cache), `ban/`, `tiles/`, `sytadin/`, `disruptions/` (and `valhalla/` for the routing tiles) |
| `gtfs_url` | URL to download the GTFS zip archive |
| `osm_url` | URL to download the OpenStreetMap PBF file (for Valhalla) |
| `ban_url` | Base URL for BAN address CSV files. Read by `bin/download.sh` only, not by the backend |
| `departments` | French department codes to download BAN data for |

```admonish warning title="Changing data.dir"
The backend and `bin/download.sh` honour `data.dir`, but `bin/valhalla.sh` (`data/osm`, `data/valhalla`), the GTFS check in `bin/start.sh` (`data/gtfs`) and `docker/docker-compose.yml` (`../data`) use `data/` under the repository root regardless.
```

`bin/download.sh` reads `data.*` and `traffic.base_url` from `config.yaml` and stops if one of them is missing.

## Routing

```yaml
routing:
  max_journeys: 5
  max_transfers: 5
  default_transfer_time: 120    # seconds
  rail_change_time: 300         # seconds, train/RER/metro ↔ train/RER/metro at one stop
  max_duration: 10800           # 3 hours in seconds
  service_day_start: 14400      # 04:00, in seconds after midnight
  max_nearest_stop_distance: 1500  # meters (~20 min walk at 5 km/h)
  fallback_stop_distance: 2500  # meters, retried when an address finds nothing
  walk_matrix_stops: 30         # nearest stops whose walk is timed by Valhalla
  diverse_lines: false          # one line per alternative (line-level diversity)
  prefer_rail: false            # rail found first, bus only fills remaining slots
  prefer_rail_max_walk: 600     # seconds — rail stop needed this close to both ends
  maneuvers: false              # include turn-by-turn directions (server-controlled)
```

| Setting | Description | Default |
|---------|-------------|---------|
| `max_journeys` | Maximum number of alternative journeys to return | `5` |
| `max_transfers` | Maximum number of transfers in a journey. Each one is a RAPTOR round, and the per-thread search tables grow with it (`(max_transfers + 2) × stops` entries), so very high values cost memory and time. | `5` |
| `default_transfer_time` | Default walking time between stops (seconds), and the time to change vehicles at a single stop | `120` |
| `rail_change_time` | Time to change between two train/RER/metro vehicles at a single stop (seconds). IDFM merges all platforms of a station into one stop with no transfer time of its own; this keeps a change at Châtelet or Gare du Nord realistic. | `300` |
| `max_duration` | Maximum total journey duration (seconds) | `10800` (3h) |
| `service_day_start` | Seconds after midnight at which the service day starts. Earlier queries are answered on the previous day's services shifted by 24 h, since GTFS files night runs under the day they started (`25:30:00`). | `14400` (04:00) |
| `max_nearest_stop_distance` | Maximum distance to nearest stops (meters) | `1500` |
| `fallback_stop_distance` | Radius (meters) of a second search, run only when an address origin or destination found no journey within `max_nearest_stop_distance` — the rural case. At or below `max_nearest_stop_distance` disables it. | `2500` |
| `walk_matrix_stops` | For an address origin or destination, how many of the nearest candidate stops get their walk timed by Valhalla before routing. Farther stops keep the straight-line estimate, scaled by the detour measured on the near ones. Higher is more accurate but slower. | `30` |
| `diverse_lines` | Force each alternative to depart on a **different line** (excludes the whole head line between iterations, not just the used pattern). Surfaces slower-but-distinct lines instead of variants of the same fast corridor. Server-controlled only. | `false` |
| `prefer_rail` | Find rail/metro/tram/train journeys **first** (buses forbidden in a first search tier); buses then only fill the remaining alternative slots. The final list is still sorted by duration. Only applied when a rail/metro/tram stop is within `prefer_rail_max_walk` of both ends. Server-controlled only. | `false` |
| `prefer_rail_max_walk` | Walk (seconds) within which a rail/metro/tram stop must lie, at both the origin and the destination, for `prefer_rail` to apply. Beyond it, forbidding buses would only trade a feeder bus for a long walk. | `600` |
| `maneuvers` | Include turn-by-turn directions (Valhalla) in walk/bike/car and transfer sections. Server-controlled only — **not** a request parameter. | `false` |

## Valhalla

```yaml
valhalla:
  host: "localhost"
  port: 8002
  pedestrian_timeout_secs: 5
```

| Setting | Description | Default |
|---------|-------------|---------|
| `host` / `port` | Valhalla HTTP API | `localhost` / `8002` |
| `pedestrian_timeout_secs` | Budget for each pedestrian call made while answering a public transport search (walk matrices, first/last-mile legs). Short on purpose: a slow Valhalla degrades to estimates and missing shapes rather than a stalled search. Bike and car directions use the HTTP client's 30 s timeout. | `5` |

The Valhalla routing engine is used for walking, cycling, and driving directions. It runs as a separate Docker container. When OSM data includes indoor information, Valhalla provides indoor maneuvers (elevator, stairs, escalator, enter/exit building) in transfer and walking sections.

Environment variables override a few settings without editing the file — used by Docker Compose, where Valhalla is the `valhalla` service rather than `localhost`, and to keep the API key out of files and images:

| Variable | Overrides |
|----------|-----------|
| `GLOVE_VALHALLA_HOST` | `valhalla.host` (ignored when empty) |
| `GLOVE_VALHALLA_PORT` | `valhalla.port` (ignored, with a warning, when not a valid port) |
| `GLOVE_API_KEY` | `server.api_key` (applied even when empty, which disables the protected endpoints) |
| `GLOVE_TILE_API_KEY` | `map.tile_api_key` |

## Traffic

```yaml
traffic:
  enabled: false
  base_url: "https://www.sytadin.fr/diffusion"
  refresh_secs: 60
```

Real-time road traffic overlay for Île-de-France, sourced from the Sytadin (DiRIF) diffusion feed. Disabled by default — but `config.yaml.sample` enables it (with `refresh_secs: 600`).

| Setting | Description | Default |
|---------|-------------|---------|
| `enabled` | Load the road geometry at startup and poll the live feed. When `false`, nothing is loaded, nothing is polled, and the traffic endpoints report `enabled: false` | `false` |
| `base_url` | Root of the diffusion feed, without a trailing slash. States are read from `{base_url}/xml/segments_dyn.xml`, events from `{base_url}/xml/evenements.xml` | Sytadin |
| `refresh_secs` | Interval between live-data refreshes. The upstream feed updates about once a minute | `60` |

The static road geometry is **not** downloaded by the server: run `bin/download.sh traffic` to fetch `Segment.mif`/`Segment.mid` into `{data.dir}/sytadin`. A missing or unreadable geometry disables the overlay with a warning instead of preventing startup.

See [Road Traffic](../api/traffic.md) for the endpoints and payloads.

```admonish info title="Data licence"
Data © Ministère chargé des transports / DiRIF — Sytadin®, subject to usage conditions.
```

## Real-time

```yaml
realtime:
  enabled: false
  feeds:
    - name: "idfm-trip-updates"
      type: gtfs-rt
      url: "https://prim.iledefrance-mobilites.fr/marketplace/gtfs-rt"
      refresh_secs: 30
      timeout_secs: 10
      headers:
        apikey: "YOUR_KEY"
```

Real-time transit data: delays and cancellations of scheduled trips, applied at query time as an overlay on the timetable. Disabled by default; `config.yaml.sample` ships it disabled with the feed above commented out.

| Setting | Description | Default |
|---------|-------------|---------|
| `enabled` | Poll the feeds and route on predicted times. When `false`, nothing is polled and routing uses the published schedule | `false` |
| `feeds` | List of feeds, merged in order (on conflict, the last one wins) | `[]` |
| `feeds[].name` | Identifier shown in logs and on `GET /api/realtime/status`. Required | — |
| `feeds[].type` | Wire format. Only `gtfs-rt` (GTFS-Realtime protobuf) exists today. Required | — |
| `feeds[].url` | Endpoint to poll. Required | — |
| `feeds[].refresh_secs` | Seconds between two polls | `30` |
| `feeds[].timeout_secs` | Per-request timeout (seconds). Keep it below `refresh_secs` so a stalled upstream cannot pile requests up | `10` |
| `feeds[].headers` | Extra request headers sent on every poll, typically the provider's API key | `{}` |

Feed credentials stay out of the logs: the startup log prints header names but not their values, and URLs with their query string redacted.

```admonish tip title="Check the matching"
A feed can answer 200 with a valid body and still match nothing when its identifiers use a different namespace than the GTFS. After adding a feed, check the per-feed health and schedule-matching counters on `GET /api/realtime/status`.
```

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
  tile_url: "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png"
  tile_cache_duration: 864000    # seconds (10 days)
```

These settings control the initial map view, geographic bounds, and the tile caching proxy.

| Setting | Description | Default |
|---------|-------------|---------|
| `zoom` | Default map zoom level | `11` |
| `center_lat` / `center_lon` | Default map center | `48.8566` / `2.3522` (Paris) |
| `bounds_sw_*` / `bounds_ne_*` | Geographic bounds (SW and NE corners) | Île-de-France |
| `tile_url` | Upstream tile server URL template. Placeholders: `{s}` (subdomain), `{z}`, `{x}`, `{y}`, `{r}` (retina), `{key}` (`tile_api_key`) | CARTO Voyager |
| `tile_api_key` | Tile provider API key, substituted for `{key}`. Overridden by `GLOVE_TILE_API_KEY` | `""` |
| `tile_cache_duration` | Browser cache duration for tiles (seconds) | `86400` (1 day; the sample sets `864000`) |

Tiles are fetched from the upstream server on first request and cached to `data/tiles/` on disk. Subsequent requests are served from cache.

For a provider that requires a key, put `{key}` where the key goes and set `GLOVE_TILE_API_KEY` (or `tile_api_key`), e.g. CARTO:

```yaml
map:
  tile_url: "https://basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png?key={key}"
```

Only the server's tile proxy sees the key: the browser requests `/api/tiles/...`, and the key is kept out of the startup log and of upstream error messages.

## Pedestrian

```yaml
pedestrian:
  walking_speed: 5.0
  step_penalty: 30
  elevator_penalty: 60
```

The default walker, used for first/last-mile walks in public transport journeys and by the walk endpoint. The wheelchair profile below replaces it when a request sets `wheelchair=true`. Station transfers ignore both penalties: stairs and elevators are the normal path through a station.

| Setting | Description | Default |
|---------|-------------|---------|
| `walking_speed` | Walking speed (km/h) when the request gives no `walking_speed`. Used both to pick candidate stops and by Valhalla, so the two agree. | `5.0` |
| `step_penalty` | Valhalla penalty (seconds) per flight of stairs | `30` |
| `elevator_penalty` | Valhalla penalty (seconds) for taking an elevator | `60` |

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

## Wheelchair Accessibility

```yaml
wheelchair:
  step_penalty: 999999          # effectively avoid stairs
  max_grade: 6                  # 6% slope max (wheelchair norms)
  use_hills: 0.0                # avoid hills entirely
  elevator_penalty: 0           # prefer elevators
  walking_speed: 3.5            # km/h — typical wheelchair speed
```

These settings are used when the `wheelchair=true` parameter is passed to journey endpoints. They configure Valhalla's pedestrian costing model for wheelchair-accessible routing.

| Setting | Description | Default |
|---------|-------------|---------|
| `step_penalty` | Penalty for stairs. Very high value effectively avoids them | `999999` |
| `max_grade` | Maximum road grade in percent (6% is the standard wheelchair norm) | `6` |
| `use_hills` | Hill avoidance factor (0.0 = strongly avoid, 1.0 = no preference) | `0.0` |
| `elevator_penalty` | Penalty for elevators (0 = prefer them) | `0` |
| `walking_speed` | Wheelchair speed in km/h | `3.5` |

```admonish info
When wheelchair mode is active, the walking speed slider in the frontend is locked at 3.5 km/h — a value hard-coded in the portal, not read from `wheelchair.walking_speed` — and the bike mode is disabled. Walk and car modes stay available.
```
