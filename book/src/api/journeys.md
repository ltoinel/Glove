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
          "geojson": { ... }
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
          "duration": 180
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
- `least_walking` — Least walking time

## Walking

```
GET /api/journeys/walk
```

Uses Valhalla for pedestrian routing.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `from` | string | Yes | Origin (`lon;lat`) |
| `to` | string | Yes | Destination (`lon;lat`) |

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
