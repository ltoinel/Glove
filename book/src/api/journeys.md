# Journey Planning

## Public Transit

```
GET /api/journeys/public_transport
```

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | string | Yes | Origin coordinates (`lon;lat`) |
| `to` | string | Yes | Destination coordinates (`lon;lat`) |
| `datetime` | string | No | Departure time (ISO 8601, e.g. `20240315T083000`). Defaults to now |
| `datetime_represents` | string | No | Whether `datetime` is `departure` (default) or `arrival` |
| `maneuvers` | bool | No | Include turn-by-turn maneuvers in response (default: `false`). When absent, maneuvers are omitted and transfer Valhalla enrichment is skipped |
| `wheelchair` | bool | No | Enable wheelchair-accessible routing (default: `false`). Avoids stairs, limits slope, prefers elevators. Adds `most_accessible` journey tag |
| `forbidden_modes` | string | No | Comma-separated commercial modes to exclude (e.g. `metro,bus,rail`) |
| `forbidden_uris[]` | string | No | Route IDs to exclude from routing |
| `walking_speed` | float | No | Walking speed override in m/s (default: ~1.12 m/s = 4 km/h) |
| `max_nb_transfers` | int | No | Maximum number of transfers allowed |
| `min_nb_transfers` | int | No | Minimum number of transfers |
| `max_duration` | int | No | Maximum journey duration in seconds |
| `max_walking_duration_to_pt` | int | No | Maximum walking time to reach transit (seconds) |
| `first_section_mode[]` | string | No | Modes allowed for the first leg (e.g. `walking`, `bike`, `car`) |
| `last_section_mode[]` | string | No | Modes allowed for the last leg |
| `direct_path` | string | No | Include direct non-transit path (`none`, `only`) |
| `count` | int | No | Number of journeys requested |
| `max_nb_journeys` | int | No | Maximum number of journeys in response |

### Example

```bash
curl "http://localhost:8080/api/journeys/public_transport?\
from=2.3522;48.8566&\
to=2.2945;48.8584&\
datetime=20240315T083000"
```

### Response

The response follows the Navitia journey format:

```json
{
  "journeys": [
    {
      "departure_date_time": "20240315T083000",
      "arrival_date_time": "20240315T090500",
      "duration": 2100,
      "nb_transfers": 1,
      "tags": ["fastest"],
      "sections": [
        {
          "type": "street_network",
          "mode": "walking",
          "duration": 300,
          "geojson": { ... },
          "maneuvers": [
            {
              "instruction": "Walk south on Rue de Rivoli.",
              "maneuver_type": 2
            }
          ]
        },
        {
          "type": "public_transport",
          "display_informations": {
            "commercial_mode": "Metro",
            "code": "1",
            "direction": "La Défense",
            "color": "FFCD00"
          },
          "from": { "name": "Châtelet", ... },
          "to": { "name": "Charles de Gaulle - Étoile", ... },
          "departure_date_time": "20240315T083500",
          "arrival_date_time": "20240315T085000",
          "geojson": { ... },
          "stop_date_times": [ ... ]
        },
        {
          "type": "transfer",
          "duration": 180,
          "maneuvers": [
            {
              "instruction": "Take the elevator to level 0.",
              "maneuver_type": 37
            }
          ]
        },
        {
          "type": "public_transport",
          ...
        }
      ]
    }
  ]
}
```

### Journey Tags

Each journey may have one or more tags:
- `fastest` — Shortest total duration
- `least_transfers` — Fewest number of transfers
- `least_walking` — Least total walking time, including both street_network sections (first/last mile) and transfer durations
- `most_accessible` — *(wheelchair mode only)* Least walking + fewest transfers, best for wheelchair users

### Maneuvers

Maneuvers are **disabled by default**. To include them, pass `?maneuvers=true` on any endpoint. When enabled, street network sections and transfer sections include a `maneuvers` array with turn-by-turn directions. Each maneuver contains:

| Field | Description |
|-------|-------------|
| `instruction` | Human-readable direction text |
| `maneuver_type` | Valhalla maneuver type number (e.g., 2 = turn right, 37 = elevator, 38 = stairs, 39 = escalator) |

Transfer sections only include maneuvers when indoor routing data is available from OSM. Indoor maneuver types include elevator (37), stairs (38), escalator (39), enter building (40), and exit building (41).

```admonish info title="Maneuver Types"
The `maneuver_type` field is a Valhalla type number included in all walk, bike, and car responses, as well as in `street_network` and `transfer` sections of public transport responses.
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
| `maneuvers` | bool | No | Include turn-by-turn maneuvers (default: `false`) |
| `wheelchair` | bool | No | Wheelchair-accessible routing: avoids stairs, limits slope to 6%, speed 3.5 km/h |

## Cycling

```
GET /api/journeys/bike
```

Uses Valhalla with configurable bike profiles.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | string | Yes | Origin (`lon;lat`) |
| `to` | string | Yes | Destination (`lon;lat`) |
| `profile` | string | No | Bike profile: `city`, `ebike`, or `road` (default: `city`) |
| `maneuvers` | bool | No | Include turn-by-turn maneuvers (default: `false`) |

```admonish info title="Elevation Colors"
The response includes elevation data and maneuver-by-maneuver directions. The frontend uses elevation data to color the route polyline (green = descent, red = climb).
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
| `maneuvers` | bool | No | Include turn-by-turn maneuvers (default: `false`) |

## Wheelchair Accessible Routing

All journey endpoints that use Valhalla (public transit, walk) support a `wheelchair=true` parameter. When enabled:

- **Stairs are avoided** — Step penalty set extremely high (999999)
- **Slope is limited** — Maximum grade 6% (wheelchair norm)
- **Hills are avoided** — Use hills factor set to 0.0
- **Elevators are preferred** — Elevator penalty set to 0
- **Speed is reduced** — Walking speed fixed at 3.5 km/h (typical wheelchair speed)

For public transit, wheelchair mode also adds the `most_accessible` journey tag to the result with the fewest transfers and least walking time.

```admonish tip
In the frontend, the wheelchair toggle in the settings panel automatically enables this mode and disables the walking speed slider (fixed at 3.5 km/h). Bike and car modes are hidden when wheelchair mode is active.
```

## Tile Caching Proxy

```
GET /api/tiles/{z}/{x}/{y}.png
```

Proxies map tile requests to a configurable upstream tile server and caches tiles locally on disk under `data/tiles/{z}/{x}/{y}.png`. Subsequent requests are served from cache.

| Parameter | Type | Description |
|-----------|------|-------------|
| `z` | integer | Zoom level (0–20) |
| `x` | integer | Tile column |
| `y` | integer | Tile row |

The upstream server URL template and browser cache duration are configured in `config.yaml`:

```yaml
map:
  tile_url: "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png"
  tile_cache_duration: 864000    # seconds (10 days)
```

Placeholders: `{s}` (subdomain a/b/c/d for load balancing), `{z}`, `{x}`, `{y}`, `{r}` (retina).

```admonish info title="Rate Limiting"
Tile requests are excluded from the per-IP rate limiting to allow smooth map panning.
```
