# Places & Autocomplete

```
GET /api/places
```

Search for transit stops and addresses with fuzzy matching.

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `q` | string | Yes | Search query (e.g. "gare de lyon") |

## Example

```bash
curl "http://localhost:8080/api/places?q=chatelet"
```

## Response

```json
{
  "places": [
    {
      "id": "stop_point:IDFM:12345",
      "name": "Châtelet",
      "embedded_type": "stop_point",
      "stop_point": {
        "id": "stop_point:IDFM:12345",
        "name": "Châtelet",
        "coord": {
          "lat": "48.858370",
          "lon": "2.347000"
        }
      }
    },
    {
      "id": "address:75001_chatelet",
      "name": "Place du Châtelet, Paris",
      "embedded_type": "address",
      "address": {
        "name": "Place du Châtelet",
        "coord": {
          "lat": "48.858200",
          "lon": "2.347100"
        }
      }
    }
  ]
}
```

## Search Ranking

Results are ranked by match quality:

1. **Exact match** — "Châtelet" matches "Châtelet" (highest priority)
2. **Prefix match** — "chat" matches "Châtelet"
3. **Word-prefix match** — "lyon" matches "Gare de Lyon"
4. **Substring match** — "elet" matches "Châtelet" (lowest priority)

Transit stops are always prioritized over BAN addresses in the results.

## Diacritics Normalization

The search handles French diacritics transparently:
- `chatelet` matches `Châtelet`
- `gare de l'est` matches `Gare de l'Est`
- `opera` matches `Opéra`
