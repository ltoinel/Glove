# Places & Autocomplete

```
GET /api/places
```

Search for transit stops and addresses with fuzzy matching. Stops come from the loaded GTFS, addresses from the BAN (Base Adresse Nationale) index.

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `q` | string | No | Search query (e.g. "gare de lyon"). Fewer than 2 characters (or absent) returns an empty list |
| `limit` | integer | No | Maximum number of results (default: 10, max: 50) |

## Example

```bash
curl "http://localhost:8080/api/places?q=chatelet&limit=3"
```

## Response

Every result is a flat object with `id`, `name`, `type` (`stop` or `address`) and `coord`:

```json
{
  "places": [
    {
      "id": "IDFM:463160",
      "name": "Châtelet",
      "type": "stop",
      "coord": { "lat": 48.85861595793795, "lon": 2.3479221142329934 }
    },
    {
      "id": "IDFM:monomodalStopPlace:45102",
      "name": "Châtelet - Les Halles",
      "type": "stop",
      "coord": { "lat": 48.86174503191322, "lon": 2.34697651043533 }
    }
  ]
}
```

An address result carries its coordinates as its `id`, in the `lon;lat` form the journey endpoints accept, so a client can pass the `id` of any result straight to `from` / `to`:

```json
{
  "id": "1.986609;48.833127",
  "name": "12 Rue de Rivoli, 78450 Villepreux",
  "type": "address",
  "coord": { "lat": 48.833127, "lon": 1.986609 }
}
```

## Search Ranking

Results are ranked by match quality:

1. **Exact match** — "Châtelet" matches "Châtelet" (highest priority)
2. **Prefix match** — "chat" matches "Châtelet"
3. **Word-prefix match** — "lyon" matches "Gare de Lyon"
4. **Substring match** — "elet" matches "Châtelet" (lowest priority)

Transit stops are always listed before BAN addresses, up to `limit` results in total.

## House Numbers

Digits are stripped from the query before searching stops, since stop names carry no street numbers; the address search receives the full query. When the query starts with a number (`12 rue de rivoli`), that number is prefixed to each address `name` and the position is that of the house number: exact when BAN knows it, otherwise interpolated between the two nearest known numbers. Without a number, the street's centroid is returned.

## Diacritics Normalization

The search handles French diacritics transparently:
- `chatelet` matches `Châtelet`
- `gare de l'est` matches `Gare de l'Est`
- `opera` matches `Opéra`
