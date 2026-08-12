# Road Traffic

Glove can overlay real-time road traffic on the map, sourced from the **Sytadin** diffusion feed published by the DiRIF (Direction Interdépartementale des Routes Île-de-France).

The overlay is **disabled by default**. See [Configuration](../getting-started/configuration.md#traffic) to enable it.

```admonish info title="Data licence"
Data © Ministère chargé des transports / DiRIF — Sytadin®, subject to usage conditions. Glove fetches it at runtime and never redistributes it.
```

## Two endpoints, split by lifetime

The road network never changes while the server runs; only the traffic states do. Sending both together would mean re-transmitting roughly 1 MB of unchanged polylines every refresh cycle, so they are served separately.

| Method | Path | Payload | Cache |
|--------|------|---------|-------|
| `GET` | `/api/traffic/geometry` | ~785 kB — road polylines | `public, max-age=86400` |
| `GET` | `/api/traffic/states` | ~175 kB — states and events | `no-cache` |

Clients fetch the geometry once and join it against the states on each refresh, by segment id.

### `GET /api/traffic/geometry`

The static road network, built once at startup from the Sytadin MIF/MID files and reprojected from Lambert II étendu to WGS84.

```json
{
  "enabled": true,
  "segments": {
    "12017889": [[48.7781, 2.4382], [48.7762, 2.4390]]
  }
}
```

Keys are Sytadin `ID_SEGMENT` values; each polyline is a list of `[lat, lon]` pairs rounded to 4 decimals (~11 m, below one pixel at every zoom level the overlay is drawn at).

### `GET /api/traffic/states`

The live snapshot, refreshed in the background every `traffic.refresh_secs`.

```json
{
  "enabled": true,
  "updated_at": "2026-08-13T01:08:17",
  "states": { "12017889": "jam" },
  "events": [
    {
      "category": "roadwork",
      "label": "Fermeture",
      "pos": [48.7781, 2.4382],
      "end": "2026-09-04T07:45:17"
    }
  ]
}
```

| Field | Notes |
|-------|-------|
| `updated_at` | The feed's own publication time (`DateDiffusion`) when it provides one — that is when the data was *measured*, not when Glove polled for it. Falls back to the server clock otherwise. |
| `states` | `fluid`, `jam` or `closed`. Segments reported as `Non renseigne` upstream, or missing from the geometry, are omitted rather than represented. |
| `events` | `category` is normalized to `roadwork`, `accident`, `jam`, `weather` or `event`. `pos` is the midpoint of the event's first located segment. `end` is present only when the feed announces an expected end. |

Responses use `503` with a `traffic_unavailable` error while the overlay is enabled but no snapshot has been fetched yet.

## Behaviour when disabled

Both endpoints answer `200` with `enabled: false` and empty collections, so a client needs no special case. The overlay reports itself disabled when `traffic.enabled` is `false`, and also when the geometry files are missing or unreadable — a broken overlay never prevents the server from starting.

## Implementation notes

- Both bodies are serialized **once** — at startup for the geometry, at each refresh for the states — and handed out verbatim. A snapshot covers several thousand segments, so serializing per request would dominate the handler's cost.
- The states body is swapped atomically with `ArcSwapOption`, the same lock-free approach used for the RAPTOR index hot-reload: readers never block and always see a complete snapshot.
- Geometry parsing applies the Paris prime meridian offset by hand. `proj4rs` adds a `+pm=paris` declaration as if it were radians, which lands results about 130° off; applying it after the datum shift instead costs ~7 m of accuracy, an order of magnitude below what a road overlay needs.
