# Disruptions & Real-Time

Two overlays change what the router answers without rebuilding the RAPTOR index: **disruptions** entered by operators (works, incidents, closures) and **real-time** delays and cancellations read from GTFS-Realtime feeds. Both are applied at query time.

## Disruptions

A disruption names a part of the network, a period and a severity. Reads are public; writes require the `X-Api-Key` header (see [Authentication](./endpoints.md#authentication)): `403` when `server.api_key` is empty, `401` when the key is missing or wrong.

The catalogue is persisted as one JSON document at `{data.dir}/disruptions/disruptions.json`, rewritten atomically on every write. The back office (`Disruptions` view of the portal) is a client of these endpoints.

### The Disruption object

```json
{
  "id": "d3",
  "title": "Travaux ligne 4",
  "message": "Service interrompu entre Montparnasse et Châtelet.",
  "cause": "works",
  "severity": "blocking",
  "scope": {
    "type": "line_section",
    "route_id": "IDFM:C01374",
    "from_stop_id": "IDFM:monomodalStopPlace:47900",
    "to_stop_id": "IDFM:monomodalStopPlace:58566"
  },
  "starts_at": "2026-09-05T22:00:00",
  "ends_at": "2026-09-08T04:30:00",
  "created_at": "2026-09-01T10:12:44",
  "updated_at": "2026-09-01T10:12:44"
}
```

| Field | Description |
|-------|-------------|
| `id` | Server-assigned (`d1`, `d2`…), stable across updates, never reused |
| `title` | One-line headline shown in journey results. Required |
| `message` | Longer explanation, may be empty |
| `cause` | `works` (default), `incident`, `strike`, `event`, `weather`, `other` |
| `severity` | `blocking` (default): the object is removed from routing. `info`: journeys touching it are annotated, nothing is removed |
| `scope` | What is disrupted, tagged by `type` (below) |
| `starts_at` | Local wall-clock date-time, `YYYY-MM-DDTHH:MM:SS` |
| `ends_at` | Same format, strictly after `starts_at`. Omitted = ongoing, no resumption announced. The period is half-open: a disruption ending at 05:00 does not affect a 05:00 departure |
| `created_at`, `updated_at` | Set by the server |

Scopes:

| `type` | Fields | Routing effect when `blocking` |
|--------|--------|-------------------------------|
| `stop` | `stop_id` | The stop is neutralized — no boarding, alighting or transfer — while vehicles still run through it. A parent station id closes the station and all its platforms |
| `line` | `route_id` | Every pattern of the line is excluded, both directions |
| `line_section` | `route_id`, `from_stop_id`, `to_stop_id` (distinct) | Rides between the two stops are cut in both directions; the rest of the line stays usable |

When a blocking disruption removes the journey a traveller would otherwise have taken, the public transport endpoint still returns it with `status: "blocked"` and the disruptions explaining why — see [Journey Planning](./journeys.md#journey-fields).

### `GET /api/disruptions`

Lists the catalogue, newest first.

| Parameter | Type | Description |
|-----------|------|-------------|
| `active_at` | string | Keep only disruptions in force at this local date-time (`YYYY-MM-DDTHH:MM:SS`), or `now`. Any other format returns `400` |
| `scope` | string | Keep only one scope type: `stop`, `line` or `line_section` |

```json
{ "disruptions": [ { "id": "d3", ... } ] }
```

### `GET /api/disruptions/{id}`

Returns one Disruption object, or `404` (`"id": "unknown_object"`).

### `POST /api/disruptions` *(API key)*

Creates a disruption. The body holds the writable fields — `title`, `message`, `cause`, `severity`, `scope`, `starts_at`, `ends_at`; `id` and the timestamps are ignored. Returns `201 Created` with the stored object.

```bash
curl -X POST http://localhost:8080/api/disruptions \
  -H "X-Api-Key: your-secret-key" \
  -H "Content-Type: application/json" \
  -d '{
        "title": "Station fermée",
        "cause": "incident",
        "scope": { "type": "stop", "stop_id": "IDFM:monomodalStopPlace:45102" },
        "starts_at": "2026-10-02T08:00:00"
      }'
```

### `PUT /api/disruptions/{id}` *(API key)*

Replaces the writable fields of an existing disruption (same body as `POST`). Returns `200` with the updated object, or `404`.

### `DELETE /api/disruptions/{id}` *(API key)*

Returns `204 No Content`, or `404`.

### Write errors

| Status | `error.id` | When |
|--------|------------|------|
| `400` | `bad_request` | Empty `title`, empty scope identifiers, identical section endpoints, `ends_at` not after `starts_at`, or a malformed body |
| `401` | `unauthorized` | Missing or wrong `X-Api-Key` |
| `403` | `disabled` | `server.api_key` is empty |
| `404` | `unknown_object` | No disruption with that id |
| `500` | `internal_error` | The catalogue could not be written to disk |

Identifiers are not checked against the loaded GTFS: an unknown `stop_id` or `route_id` is stored but blocks nothing.

### `GET /api/disruptions/active`

The **blocking** disruptions in force right now, resolved to map geometry for the portal's blockage overlay. `info` disruptions are left out — they remove nothing.

```json
{
  "resolved_at": "2026-10-02T08:15:00",
  "disruptions": [
    {
      "id": "d3",
      "title": "Travaux ligne 4",
      "message": "Service interrompu entre Montparnasse et Châtelet.",
      "cause": "works",
      "scope": "line_section",
      "starts_at": "2026-09-05T22:00:00",
      "ends_at": "2026-09-08T04:30:00",
      "stops": [],
      "segments": [
        [[48.8442, 2.3236], [48.8462, 2.3290]]
      ]
    }
  ]
}
```

| Field | Description |
|-------|-------------|
| `scope` | `stop`, `line` or `line_section` |
| `stops` | Closed stops (`id`, `name`, `lat`, `lon`). Empty for a line or section closure |
| `segments` | Cut rides as `[[lat, lon], [lat, lon]]` pairs (WGS84), deduplicated across patterns and directions. Straight lines between consecutive stops — schematic, not the track alignment |

`message` is omitted when empty, `ends_at` when no end is announced.

## Lines

```
GET /api/lines
```

The line catalogue of the loaded GTFS, used by the back-office line pickers.

| Parameter | Type | Description |
|-----------|------|-------------|
| `q` | string | Case- and accent-insensitive substring filter on the short or long name |
| `limit` | integer | Maximum number of results (default 50, max 200) |

```json
{
  "lines": [
    {
      "id": "IDFM:C01849",
      "short_name": "A",
      "long_name": "Remplacement RER A",
      "mode": "bus",
      "color": "EB2132",
      "text_color": "FFFFFF"
    }
  ]
}
```

`mode` is the commercial mode (`metro`, `rail`, `tramway`, `bus`, `funicular`, `other`). Results are sorted by short name, purely numeric names first in numeric order (`1` before `10`).

## Real-Time Status

```
GET /api/realtime/status
```

Health of the configured real-time feeds (`realtime.feeds` in `config.yaml`) and how well their data matched the schedule. A feed can answer `200` with a valid body and still match nothing when its identifiers differ from the GTFS — the matching counters make that visible.

```json
{
  "enabled": true,
  "published_at": "2026-10-02T06:14:30.512304+00:00",
  "updated_trips": 1834,
  "matching": {
    "matched_trips": 2210,
    "unmatched_trips": 37,
    "canceled_trips": 12,
    "unsupported_trips": 3,
    "matched_calls": 40122,
    "unmatched_calls": 88,
    "sequence_fallback_calls": 0,
    "unresolved_times": 0
  },
  "feeds": [
    {
      "name": "idfm",
      "kind": "gtfs-rt",
      "url": "https://example.org/gtfs-rt/trip-updates?…",
      "refresh_secs": 30,
      "last_success": "2026-10-02T06:14:29.871022+00:00",
      "last_error": null,
      "trip_updates": 2262
    }
  ]
}
```

| Field | Description |
|-------|-------------|
| `enabled` | Whether real-time routing is configured and running. With no feed: `false`, zero counters and an empty `feeds` |
| `published_at` | When the current overlay was published (RFC 3339, UTC). Omitted before the first one |
| `updated_trips` | Trips currently carrying a delay or a cancellation |
| `matching` | Counters of the last refresh: trip updates resolved to a scheduled trip or not, cancellations, unsupported (`ADDED`) trips, calls resolved to a position in their pattern or not, calls located by `stop_sequence` for lack of a `stop_id`, and absolute times that could not be converted |
| `feeds` | One entry per configured feed, in configuration order. `url` has its query string replaced by `…`, since it often carries an API key; `last_success` is RFC 3339 UTC; `last_success` / `last_error` are `null` until known |
