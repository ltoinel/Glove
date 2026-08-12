# Data Integration

This page traces the Sytadin road traffic data end to end: what the feed publishes, how Glove ingests it, and what it costs. For the resulting HTTP contract, see [Road Traffic](../api/traffic.md); for the settings, see [Configuration](../getting-started/configuration.md#traffic).

```admonish info title="Source and licence"
Sytadin is published by the **DiRIF** (Direction Interdépartementale des Routes Île-de-France). Data © Ministère chargé des transports / DiRIF — Sytadin®, subject to usage conditions. Glove downloads the static geometry into `data/sytadin/` and fetches the dynamic feeds at runtime; neither is redistributed.
```

## What the feed publishes

The diffusion root exposes far more than Glove currently consumes.

| File | Content | Used |
|------|---------|:----:|
| `mifmid/modelisation/Segment.mif` / `.mid` | Road network geometry, 9 140 segments | ✅ |
| `xml/segments_dyn.xml` | Segment traffic states | ✅ |
| `xml/evenements.xml` | Incidents, roadworks, closures | ✅ |
| `xml/arcs_dyn.xml` | Arc-level data: **speeds, travel times, statistical reference** | ❌ |
| `tpsparcours/tpsParcours.xml` | Travel times on 151 named itineraries | ❌ |
| `xml/indices.xml` | Regional congestion, volume, average-speed indices | ❌ |
| `xml/Chantier.xml`, `xml/dysfonctionnements.xml` | Worksites, malfunctions | ❌ |
| `xsd/*.xsd` | Schemas documenting every field | — |

Each family also ships an XSD that documents its fields, which is the authoritative reference when a value looks ambiguous.

## Static geometry

Downloaded once by `bin/download.sh traffic`, parsed once at startup by `src/traffic.rs`.

### MIF/MID pairing

MapInfo splits a layer in two files: the MIF holds graphic objects, the MID holds attribute rows, and the two align **row by row** — the k-th polyline belongs to the k-th attribute row. Glove parses both and zips them, logging a warning on a length mismatch.

The pairing is exact on the current data: 9 140 `pline` objects against 9 140 CSV records. The MID cannot be counted by lines, though — quoted fields contain embedded newlines, so `wc -l` reports 14 567. It must be read with a real CSV parser.

### Reprojection

The MIF header declares `Projection 3, 1002`: Lambert conformal conic on the **NTF Paris** datum, i.e. EPSG:27572 (Lambert II étendu). Leaflet needs WGS84.

```admonish warning title="proj4rs and the Paris meridian"
Declaring `+pm=paris` in the projection string produces coordinates about **130° off**. `proj4rs` adds the prime-meridian offset as if its value were in radians, when it is expressed in degrees (2.337 229 167°). Glove therefore omits `+pm=paris` and adds the offset by hand after the datum shift.

Applying it after rather than before the shift costs ~7 m of eastward accuracy — measured against an independent implementation of the inverse conic plus Helmert transform, and an order of magnitude below what a road overlay needs.
```

Coordinates are then rounded to 4 decimals (~11 m). One map pixel spans ~50 m at the default zoom and ~1.5 m at the deepest, so the rounding stays sub-pixel everywhere the overlay is drawn while removing 9 % of the payload.

A sanity check on the full dataset: 9 140 segments, 39 677 vertices, bounding box latitude 48.43–49.21, longitude 1.58–3.46 — Île-de-France exactly.

## Dynamic feeds

Polled every `traffic.refresh_secs` by the loop in `src/api/traffic.rs`, parsed as a stream with `quick-xml` (no DOM, no intermediate allocation for the whole document).

### States

`EtatTrafic` maps to three slugs; `Non renseigne` is **dropped rather than represented**, as is any segment absent from the geometry — a client is never handed a state it could not draw.

| Feed value | Exposed as |
|------------|------------|
| `Fluide` | `fluid` |
| `Bouchon` | `jam` |
| `Ferme` | `closed` |
| `Non renseigne` | *omitted* |

### Events

Each `<Evenement>` is accumulated field by field, then located at the midpoint of its first known segment; an event whose segments are all unknown is skipped, since there is nowhere to place it. `QualificationTypeEvenement` is normalized to `roadwork`, `accident`, `jam`, `weather` or `event`.

```admonish note title="Streaming pitfall"
A streaming parser must clear its current tag on **closing** elements. Without that, the whitespace between `</Tag>` and the next `<Tag>` is read as text still belonging to the tag just closed, and overwrites the value captured a moment earlier. The symptom is subtle: every event comes out uncategorized with a blank label, while the XML itself is perfectly valid.
```

### Publication time

The dynamic feeds stamp their root element with `DateDiffusion`. Glove exposes it as `updated_at` in preference to its own clock, so the user reads **when the data was measured**, not when the server happened to poll — a gap that reaches `refresh_secs` in the worst case. The attribute is not declared in every schema, so its absence falls back to the server clock.

## Serving model

The two halves have opposite lifetimes, and were split accordingly.

| | Geometry | States |
|---|---|---|
| Changes | across restarts | every minute |
| Serialized | once, at startup | once, per refresh |
| Size | 785 kB (226 kB gzipped) | 175 kB (27 kB gzipped) |
| `Cache-Control` | `public, max-age=86400` | `no-cache` |

Sending both together meant re-transmitting the polylines each cycle: **1 010 kB per refresh instead of 175 kB**. Neither body is ever serialized per request — a snapshot spans thousands of segments, which would otherwise dominate the handler's cost. The states body is swapped atomically through `ArcSwapOption`, the same lock-free approach as the RAPTOR index hot-reload.

Failure is always degradation, never an outage: missing geometry, unreadable files or an unreachable feed leave the server running and the endpoints answering `enabled: false`.

## Rendering

The portal fetches the geometry once per session, refreshes the states on a timer, and joins them by segment id.

With ~9 000 segments averaging **4.34 vertices** — 37 % of them straight two-point lines, 41 vertices at most — the rendering cost lies in the number of objects, not in geometric complexity. Simplification algorithms have nothing to remove here. Segments are therefore grouped into **one multi-polyline per state**: three Leaflet layers instead of nine thousand, drawn on a canvas renderer, with congestion painted above free-flowing traffic. Screen-space reduction is left to Leaflet's own `smoothFactor`.

## What the unused feeds hold

`arcs_dyn.xml` covers 3 622 arcs and carries, per arc, an instantaneous speed in km/h, a current travel time, and a **statistical reference travel time computed over 13 months for the same day type and time slot**, with its standard deviation and a confidence index. That reference makes a measured congestion factor possible — `TPBride / TPReference` — where the segment feed only allows a heuristic.

Two measurements taken ten minutes apart on a quiet night, however, temper the prospect:

| Observation | Value |
|---|---|
| Arcs carrying a speed | 158 / 3 622 (4.4 %) |
| Among `Bouchon` arcs | **0 / 15** |
| Among `Ferme` arcs | 0 / 251 |
| Among `Fluide` arcs | 126 / 2 569 (4.9 %) |
| Set stability between two snapshots | 90 % overlap |
| `IndiceConfiance > 0` vs speed present | 158 / 158 — exact match |

Speeds are published where a **ground sensor measures them with sufficient confidence**, not where traffic slows down: the measured subset is stable over time, and `IndiceConfiance` is the reliable flag, not `Vitesse > 0`. The plausible mechanism — inductive loops need vehicle passages to estimate a speed, and a quiet night provides too few — implies that the 4.4 % is a floor rather than a ceiling, and that peak hours should raise it. That remains a hypothesis inferred from the data's shape, not a documented fact.

```admonish question title="The open question"
If congested arcs still carry no speed at peak hours, the feed would be richest exactly where traffic is free-flowing and silent where it is not — the opposite of what an ETA correction needs. The state × measurement cross-tabulation at 8 am decides whether feeding this into the routing engine is worth building.
```
