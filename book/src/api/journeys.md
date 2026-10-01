# Journey Planning

## Public Transit

```
GET /api/journeys/public_transport
```

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | string | Yes | Origin: a `stop_id`, or `lon;lat` coordinates for an address |
| `to` | string | Yes | Destination: a `stop_id`, or `lon;lat` coordinates for an address |
| `datetime` | string | No | Departure time (ISO basic, e.g. `20240315T083000`). Defaults to now |
| `max_duration` | int | No | Maximum journey duration in seconds. Falls back to `routing.max_duration` |
| `walking_speed` | float | No | Walking speed in km/h for first/last-mile legs. Defaults to `pedestrian.walking_speed` (5) |
| `forbidden_modes` | string | No | Comma-separated commercial modes to exclude: `metro`, `rail`, `tramway`, `bus`, `funicular`, `other` (e.g. `metro,bus`) |
| `wheelchair` | bool | No | Enable wheelchair-accessible routing (default: `false`). Avoids stairs, limits slope, prefers elevators. Adds `most_accessible` journey tag |
| `language` | string | No | Language for maneuver instructions (e.g. `fr-FR`, `en-US`) |

> **Server-controlled settings.** The number of journeys (`routing.max_journeys`), transfers (`routing.max_transfers`), change times (`routing.default_transfer_time`, `routing.rail_change_time`), search radii (`routing.max_nearest_stop_distance`, `routing.fallback_stop_distance`), line diversity (`routing.diverse_lines`), rail preference (`routing.prefer_rail`, `routing.prefer_rail_max_walk`) and turn-by-turn maneuvers (`routing.maneuvers`) are **not** request parameters — they are fixed in `config.yaml`. See [Configuration](../getting-started/configuration.md). Maneuvers are likewise config-controlled on the `walk`, `bike` and `car` endpoints.

### Addresses as origin or destination

When `from` or `to` is a `lon;lat` coordinate, Glove:

1. Collects the stops within `routing.max_nearest_stop_distance` (1500 m) of it.
2. Times the walk to the `routing.walk_matrix_stops` (30) nearest of them with a single Valhalla pedestrian matrix — one call per endpoint, both in parallel. Farther candidates keep a straight-line estimate, scaled by the largest detour Valhalla measured on the near ones.
3. Runs RAPTOR on those walking times, so the first and last stops are chosen on real street distances, and the journey never leaves before the requested time.
4. If nothing is found, retries once within `routing.fallback_stop_distance` (2500 m) — the rural case where the only useful stop is a long walk away.

If Valhalla is unreachable, the straight-line estimates are used as they are.

### Example

```bash
curl "http://localhost:8080/api/journeys/public_transport?\
from=2.3522;48.8566&\
to=2.2945;48.8584&\
datetime=20240315T083000"
```

### Errors

| Status | `error.id` | When |
|--------|------------|------|
| `400` | `bad_request` | `from` or `to` missing, or `datetime` not in `YYYYMMDDTHHmmss` format |
| `400` | `unknown_object` | No stop found within the search radius of `from` or `to` (or an unknown `stop_id`) |
| `500` | `internal_error` | The search failed on the blocking thread pool |

A query that is valid but finds no route answers `200` with an empty `journeys` array.

### Response

Trimmed from a real response (`shape` and `stop_date_times` shortened):

```json
{
  "journeys": [
    {
      "departure_date_time": "20261002T083737",
      "arrival_date_time": "20261002T091721",
      "duration": 2384,
      "nb_transfers": 0,
      "tags": ["fastest", "least_transfers", "least_waiting"],
      "status": "usable",
      "sections": [
        {
          "type": "street_network",
          "from": { "id": "2.3522;48.8566", "name": "" },
          "to": {
            "id": "IDFM:monomodalStopPlace:44877",
            "name": "Saint-Michel Notre-Dame",
            "stop_point": {
              "id": "IDFM:monomodalStopPlace:44877",
              "name": "Saint-Michel Notre-Dame",
              "coord": { "lon": 2.344959, "lat": 48.853023 }
            }
          },
          "departure_date_time": "20261002T083737",
          "arrival_date_time": "20261002T085130",
          "duration": 833,
          "shape": "ag}d|AmbqnCnCqQ~Ew[^...",
          "distance": 1068
        },
        {
          "type": "public_transport",
          "from": { "id": "IDFM:monomodalStopPlace:44877", "name": "Saint-Michel Notre-Dame", "stop_point": { ... } },
          "to": { "id": "IDFM:monomodalStopPlace:58757", "name": "Champ de Mars Tour Eiffel", "stop_point": { ... } },
          "departure_date_time": "20261002T085130",
          "arrival_date_time": "20261002T090050",
          "duration": 560,
          "display_informations": {
            "network": "",
            "direction": "NORA",
            "commercial_mode": "rail",
            "label": "C",
            "color": "FFCC30",
            "text_color": "000000"
          },
          "stop_date_times": [
            {
              "stop_point": { "id": "IDFM:monomodalStopPlace:44877", "name": "Saint-Michel Notre-Dame", "coord": { ... } },
              "arrival_date_time": "20261002T085040",
              "departure_date_time": "20261002T085130"
            }
          ]
        },
        {
          "type": "street_network",
          ...
        }
      ]
    }
  ]
}
```

#### Journey fields

| Field | Description |
|-------|-------------|
| `departure_date_time`, `arrival_date_time` | Local times, `YYYYMMDDTHHmmss` |
| `duration` | Total duration in seconds, door to door |
| `nb_transfers` | Number of vehicle changes |
| `tags` | Quality tags (see below). Always empty on a blocked journey |
| `status` | `usable`, or `blocked` when a blocking disruption makes the journey impossible. A blocked journey is still returned, to explain why the obvious route is missing |
| `disruptions` | Disruptions touching the journey. Omitted when there are none |
| `sections` | Ordered legs of the journey |

Each entry of `disruptions` has `id`, `title`, `message` (omitted when empty), `cause` (`works`, `incident`, `strike`, `event`, `weather`, `other`), `severity` (`blocking` or `info`), `scope` (`stop`, `line` or `line_section`), `section` (index of the affected section), `stop_point` (only when the disruption names a stop), `starts_at` and `ends_at` (local `YYYY-MM-DDTHH:MM:SS`; `ends_at` omitted when no end is announced). See [Disruptions & Real-Time](./disruptions.md).

#### Section fields

| Field | Present on | Description |
|-------|------------|-------------|
| `type` | all | `street_network` (first/last-mile walk), `public_transport` or `transfer` |
| `from`, `to` | all | Place: `id`, `name`, and `stop_point` (`id`, `name`, `coord: {lon, lat}`) when it is a stop. An address endpoint has `id` = `"lon;lat"`, an empty `name` and no `stop_point` |
| `departure_date_time`, `arrival_date_time`, `duration` | all | Local times and duration in seconds |
| `display_informations` | `public_transport` | `network`, `direction` (trip headsign), `commercial_mode` (`metro`, `rail`, `tramway`, `bus`, `funicular` or `other`), `label` (line short name), `color`, `text_color` (hex, without `#`) |
| `stop_date_times` | `public_transport` | Every stop served: `stop_point`, `arrival_date_time`, `departure_date_time` |
| `delay` | `public_transport` | Seconds late at the arrival stop, when a real-time feed covers the leg. Absent means no feed reported on it (not the same as `0`, on time) |
| `shape` | `street_network`, `transfer` | Encoded polyline (Valhalla precision 6) of the walk, when Valhalla returned one |
| `distance` | `street_network`, `transfer` | Walk distance in meters, when Valhalla returned one |
| `transfer_type` | `transfer` | `indoor` (same station) or `outdoor` (between different stations) |
| `maneuvers` | `street_network`, `transfer` | Turn-by-turn directions, only when `routing.maneuvers` is enabled |

### Journey Tags

Each journey may have one or more tags:
- `fastest` — Shortest total duration
- `least_transfers` — Fewest number of transfers
- `least_walking` — Least total walking time, including both street_network sections (first/last mile) and transfer durations
- `least_waiting` — Least total platform waiting time (end-to-end duration minus time spent in sections)
- `most_accessible` — *(wheelchair mode only)* Least walking + fewest transfers, best for wheelchair users

Tags are computed over usable journeys only: a `blocked` journey never carries one.

### Maneuvers

Maneuvers are **server-controlled** via `routing.maneuvers` in `config.yaml` (disabled by default) — they are not a request parameter. When enabled, street network sections and transfer sections include a `maneuvers` array with turn-by-turn directions. Each maneuver contains:

| Field | Description |
|-------|-------------|
| `instruction` | Human-readable direction text |
| `type` | Valhalla maneuver type number (e.g. 10 = turn right, 15 = turn left, 39 = elevator, 40 = stairs, 41 = escalator) |
| `distance` | Length in meters |
| `duration` | Duration in seconds |
| `begin_shape_index` | Index into the section's decoded `shape` where the maneuver begins |

Transfers are routed by Valhalla too: indoor transfers (within one station) with zero stair and elevator penalties, so the in-station path is preferred. Indoor maneuver types, from Valhalla's maneuver enumeration, are elevator (39), stairs (40), escalator (41), enter building (42) and exit building (43); they only appear when the OSM data maps the station's interior.

```admonish info title="Maneuver Types"
The `type` field is the Valhalla maneuver type number, used identically in walk, bike and car responses and in the `street_network` and `transfer` sections of public transport responses.
```

## Walking

```
GET /api/journeys/walk
```

Uses Valhalla for pedestrian routing.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | string | Yes | Origin (`lon;lat`) |
| `to` | string | Yes | Destination (`lon;lat`) |
| `walking_speed` | float | No | Walking speed in km/h. Defaults to `pedestrian.walking_speed` (5) |
| `wheelchair` | bool | No | Wheelchair-accessible routing (see below) |
| `language` | string | No | Language for maneuver instructions (e.g. `fr-FR`, `en-US`) |

### Response

```json
{
  "journeys": [
    {
      "duration": 608,
      "distance": 763,
      "shape": "ag}d|AmbqnCnCqQ~Ew[^cCj@wD...",
      "maneuvers": [ ... ]
    }
  ]
}
```

`duration` is in seconds, `distance` in meters, `shape` an encoded polyline (precision 6). `maneuvers` (same fields as above) is present only when `routing.maneuvers` is enabled.

## Cycling

```
GET /api/journeys/bike
```

Uses Valhalla. The response returns **three journeys** — one per bike profile (`city`, `ebike`, `road`, configured under `bike.*` in `config.yaml`) — computed server-side; there is no profile parameter.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | string | Yes | Origin (`lon;lat`) |
| `to` | string | Yes | Destination (`lon;lat`) |
| `language` | string | No | Language for maneuver instructions (e.g. `fr-FR`, `en-US`) |

### Response

```json
{
  "journeys": [
    {
      "type": "city",
      "duration": 1696,
      "distance": 5935,
      "elevation_gain": 138,
      "elevation_loss": 142,
      "shape": "ag}d|AmbqnCnCqQ~Ew[^...",
      "heights": [35.0, 35.2, ...],
      "maneuvers": [ ... ]
    },
    { "type": "ebike", ... },
    { "type": "road", ... }
  ]
}
```

`elevation_gain` / `elevation_loss` are in meters; `heights` are elevation samples (meters) along the route, from Valhalla's `/height` service. `maneuvers` is present only when `routing.maneuvers` is enabled.

```admonish info title="Elevation Colors"
The frontend uses the elevation samples to color the route polyline (green = descent, red = climb).
```

## Driving

```
GET /api/journeys/car
```

Uses Valhalla for driving directions.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | string | Yes | Origin (`lon;lat`) |
| `to` | string | Yes | Destination (`lon;lat`) |
| `language` | string | No | Language for maneuver instructions (e.g. `fr-FR`, `en-US`) |

The response has the same shape as the walking one: `{"journeys": [{"duration", "distance", "shape", "maneuvers"?}]}`.

### Errors (walk, bike, car)

| Status | `error.id` | When |
|--------|------------|------|
| `400` | `bad_request` | `from` or `to` not in `lon;lat` format |
| `502` | `valhalla_error` | Valhalla unreachable, returned an error, or returned no route |

## Wheelchair Accessible Routing

The public transit and walk endpoints support a `wheelchair=true` parameter. When enabled, the pedestrian costing switches to the `wheelchair` profile of `config.yaml`, whose defaults are:

- **Stairs are avoided** — Step penalty set extremely high (`step_penalty: 999999`)
- **Slope is limited** — Maximum grade 6% (`max_grade`, wheelchair norm)
- **Hills are avoided** — `use_hills: 0.0`
- **Elevators are preferred** — `elevator_penalty: 0`
- **Speed is reduced** — Walking speed fixed at `walking_speed: 3.5` km/h, overriding any `walking_speed` request parameter

For public transit, wheelchair mode also adds the `most_accessible` journey tag to the result with the fewest transfers and least walking time.

```admonish tip
In the frontend, the wheelchair toggle in the settings panel automatically enables this mode and disables the walking speed slider (fixed at 3.5 km/h). Bike and car modes are hidden when wheelchair mode is active.
```

## Tile Caching Proxy

```
GET /api/tiles/{z}/{x}/{y}.png
```

Proxies map tile requests to a configurable upstream tile server and caches tiles locally on disk under `{data.dir}/tiles/{z}/{x}/{y}.png` (`data/tiles/` by default). Subsequent requests are served from cache.

| Parameter | Type | Description |
|-----------|------|-------------|
| `z` | integer | Zoom level (0–20) |
| `x` | integer | Tile column (`0` to `2^z - 1`) |
| `y` | integer | Tile row (`0` to `2^z - 1`) |

Out-of-range coordinates return `400`; an unreachable or failing upstream returns `502` (`tile_error`). Tiles are served as `image/png` with `Cache-Control: public, max-age={tile_cache_duration}`.

The upstream server URL template, API key and browser cache duration are configured in `config.yaml`:

```yaml
map:
  tile_url: "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png"
  tile_api_key: ""               # substituted for {key}; prefer GLOVE_TILE_API_KEY
  tile_cache_duration: 864000    # seconds (10 days; built-in default 86400)
```

Placeholders:

| Placeholder | Replaced by |
|-------------|-------------|
| `{s}` | Subdomain `a`/`b`/`c`/`d`, chosen from the tile coordinates for load balancing |
| `{z}`, `{x}`, `{y}` | Tile coordinates |
| `{r}` | Always an empty string: the proxy fetches standard-resolution tiles |
| `{key}` | `map.tile_api_key`, overridable by the `GLOVE_TILE_API_KEY` environment variable |

The key stays server-side: only the proxy uses it, and it is redacted from logs, so it never reaches the browser. A keyed provider looks like `https://example.com/{z}/{x}/{y}{r}.png?key={key}`.

```admonish info title="Rate Limiting"
Tile requests are excluded from the per-IP rate limiting to allow smooth map panning.
```
