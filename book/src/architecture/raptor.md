# RAPTOR Algorithm

RAPTOR (**R**ound-b**A**sed **P**ublic **T**ransit **O**ptimized **R**outer) is the core routing algorithm in Glove. It finds Pareto-optimal journeys that minimize both arrival time and number of transfers.

## How It Works

### Rounds

RAPTOR operates in rounds. Each round allows one additional vehicle trip:

- **Round 0**: Walk from the origin to nearby stops
- **Round 1**: Take one transit vehicle (no transfers)
- **Round 2**: Take up to two vehicles (one transfer)
- **Round k**: Take up to k vehicles (k-1 transfers)

The algorithm stops when no improvement is found or `max_transfers` is reached.

### Within Each Round

For each round, RAPTOR:

1. **Collects marked stops** — stops that were improved in the previous round
2. **Scans patterns** — for each route pattern passing through a marked stop, finds the earliest trip that can be boarded and propagates arrival times along the remaining stops
3. **Transfers** — from every newly improved stop, walks to neighboring stops using the transfer graph

### Labels

Each stop maintains a **label** per round: the earliest known arrival time. Labels also store back-pointers for journey reconstruction (which trip, which boarding stop, etc.).

## Pre-Processing

On startup (10-30 seconds), Glove builds several indexes:

| Index | Purpose |
|-------|---------|
| **Stop spatial index** | Maps coordinates to nearby stops within `max_nearest_stop_distance` |
| **Service ID interning** | Converts string service IDs to integers for fast calendar lookups |
| **Pattern grouping** | Groups trips with identical stop sequences into patterns |
| **Transfer graph** | Precomputes walking transfers between nearby stops |
| **Calendar index** | Maps dates to active service IDs using GTFS calendar + calendar_dates |

The index is serialized to disk (`data/raptor/`) with a fingerprint. On subsequent startups, if the GTFS data hasn't changed, the cached index is loaded directly (sub-second startup).

## Diverse Alternatives

Glove returns multiple alternative journeys using **iterative pattern exclusion**:

1. Run RAPTOR and collect all Pareto-optimal journeys from the result
2. For each journey found, record which patterns were used
3. Run RAPTOR again, excluding previously used patterns
4. Repeat until `max_journeys` is reached or no new journeys are found

This ensures diverse alternatives that use genuinely different routes, not just minor time variations.

## Service Filtering

RAPTOR is calendar-aware. For each query date:

- The active services are determined from `calendar.txt` (day-of-week rules + date ranges)
- Exceptions from `calendar_dates.txt` are applied (additions and removals)
- Only trips belonging to active services are considered during the scan

## Fuzzy Stop Search

The autocomplete endpoint uses a ranked fuzzy search with French diacritics normalization:

1. **Exact match** (highest priority)
2. **Prefix match** (stop name starts with query)
3. **Word-prefix match** (any word in the stop name starts with query)
4. **Substring match** (query appears anywhere in the stop name)

Diacritics are normalized (e.g., "gare de l'est" matches "Gare de l'Est") via a custom normalization function.

## Key Optimizations

- **Binary search** in `find_earliest_trip` for O(log n) trip lookup within patterns
- **Pre-allocated buffers** reused across rounds (no per-round allocation)
- **FxHashMap** for stop index and calendar exceptions (faster than default HashMap)
- **Pareto-optimal exploitation** — all Pareto-optimal journeys from a single RAPTOR run are used before re-running
- **Cache persistence** — serialized index with fingerprint-based invalidation
