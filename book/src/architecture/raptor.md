# RAPTOR Algorithm

RAPTOR (**R**ound-b**A**sed **P**ublic **T**ransit **O**ptimized **R**outer) is the core routing algorithm in Glove. It finds Pareto-optimal journeys that minimize both arrival time and number of transfers.

## How It Works

### Rounds

RAPTOR operates in rounds. Each round allows one additional vehicle trip:

- **Round 0**: Walk from the origin to nearby stops
- **Round 1**: Take one transit vehicle (no transfers)
- **Round 2**: Take up to two vehicles (one transfer)
- **Round k**: Take up to k vehicles (k-1 transfers)

The algorithm stops when no improvement is found or `max_transfers` is reached. `routing.max_transfers` is the only bound on the number of rounds: the per-thread search tables hold `(max_transfers + 2) × stops` entries each, so very high values cost memory and time.

### Within Each Round

For each round, RAPTOR:

1. **Collects marked stops** — stops that were improved in the previous round
2. **Scans patterns** — for each route pattern passing through a marked stop, walks along its stops. At each stop it first **alights** from the trip held since an earlier stop (recording the arrival if it improves), then tries to **board** an earlier trip
3. **Transfers** — from every stop a vehicle improved in this round, walks to neighboring stops using the transfer graph (only from those: chaining foot transfers within a round is not allowed)

Alighting before boarding is the reference RAPTOR order, and it matters: boarding first would let a trip "arrive" at the very stop it was boarded at, at its *arrival* time — which precedes its departure by the dwell. The traveller would gain that minute and catch vehicles that had already left.

### Changing vehicles

Boarding at a stop reached by walking needs no extra time: the foot transfer already counts it. Boarding at the stop a vehicle was just left at does:

| Change | Time | Setting |
|--------|------|---------|
| Train/RER/metro → train/RER/metro | 300 s | `routing.rail_change_time` |
| Any other change (bus, tram) | 120 s | `routing.default_transfer_time` |
| Same run split into two trip ids (terminus → first stop, same route) | 0 | — |

The rail value matters for Ile-de-France: IDFM merges every platform of a station into one `monomodalStopPlace` (all RER lines at Gare du Nord share one stop), and `transfers.txt` gives it no time of its own, while changes between distinct stops run 4–8 minutes. Heavy rail is GTFS route type 1, 2, 100–199 and 400–499.

### Labels

For each round, every stop keeps its earliest known arrival time (`tau`) and a **label** recording how it was reached — the trip boarded (pattern, trip, boarding and alighting positions) or the foot transfer taken. Labels are the back-pointers used for journey reconstruction; they store `u32` indices to halve the size of the largest table.

### Finding the trip to board

Trips in a pattern are sorted by departure at the first stop, and each pattern stores, per position, the minimum and maximum of `departure(pos) - departure(0)` over its trips (`departure_offsets`). `find_earliest_trip` uses these bounds for an exact binary search (trips that leave too early) and an exact early break (no later trip can improve on the best one found), at any position along the line and even when trips overtake each other.

On a pattern whose trips never overtake (`Pattern::fifo`, computed at build time), a stop reached after the held trip's departure skips the boarding search entirely — the standard RAPTOR pruning. It is disabled on a pattern touched by the real-time overlay, since delays can reorder vehicles.

### Pruning

Production searches are **bounded** (`raptor_query_bounded`): an arrival is only recorded if it is earlier than `min(departure + max_duration, best arrival at a target + its walk)`. Once a target is reached, the bound tightens after every round, and a pattern scan stops as soon as it passes it.

## Pre-Processing

On startup (10-30 seconds), Glove builds several indexes:

| Index | Purpose |
|-------|---------|
| **Stop index** | Assigns a numeric index to every `stop_id` (`build_stop_index`) |
| **Service ID interning** | Converts string service IDs to integers, so a query computes a per-service "active today" bitmap once (`intern_services`) |
| **Pattern grouping** | Groups trips with identical stop sequences into patterns, sorted by first departure, with their departure offset bounds and FIFO flag (`build_patterns`) |
| **Stop → patterns** | For each stop, the patterns serving it and the position within each |
| **Transfer graph** | Explicit `transfers.txt` links (their `min_transfer_time`, else `default_transfer_time`), plus implicit links between stops sharing a `parent_station`, timed by a Dijkstra over `pathways.txt` when the station has pathways, else `default_transfer_time` (`build_transfers`) |
| **Calendar exceptions** | `calendar_dates.txt` indexed by service then date, consulted with `calendar.txt` when a query computes its active services |
| **Search index** | Normalized names of served stops, sorted and deduplicated, for autocomplete (`build_search_index`) |

There is no spatial index: finding the stops near a coordinate (within `max_nearest_stop_distance`, or `fallback_stop_distance` on a retry) is a linear scan over the stops with a bounding-box pre-filter, plus the sibling platforms of every station found.

The index is serialized to disk (`data/raptor/`) with a fingerprint and a format version. On subsequent startups, if the GTFS data hasn't changed, the cached index is loaded directly (sub-second startup).

## Diverse Alternatives

Glove returns multiple alternative journeys using **iterative pattern exclusion** (`run_iterative_search`):

1. Run RAPTOR and build a journey from every Pareto-optimal arrival of the result
2. Keep the journeys that are not duplicates of one already found (same stops and lines), and record which patterns they used
3. Run RAPTOR again, excluding previously used patterns
4. Repeat until `max_journeys` is reached or an iteration finds nothing new

With `routing.diverse_lines`, the whole first line of each journey (every pattern of its route) is excluded as well, so each alternative departs on a different line.

This ensures diverse alternatives that use genuinely different routes, not just minor time variations.

## Access and Egress Walks

For an address origin or destination, the walk to each candidate stop is timed **before** RAPTOR runs, by a Valhalla pedestrian matrix on the `routing.walk_matrix_stops` nearest stops. A straight line undershoots real streets by a third or more (a stop 1.6 km away can be a 29-minute walk), and RAPTOR picks the first stop on these times: on the estimate alone, it boarded trains the traveller could not reach, and the journey shown left before the requested time.

Stops outside the matrix keep the straight-line estimate at `pedestrian.walking_speed`, scaled by the **largest** detour measured on the timed ones — an overestimated walk costs a wait on the platform, an underestimated one a missed train.

## Rail Preference

With `routing.prefer_rail`, a first search tier forbids buses so rail journeys are found first. It only applies when a rail, metro or tram stop lies within `routing.prefer_rail_max_walk` (600 s) of **both** ends: the tier forbids buses everywhere, including to reach the station, so without a station nearby it only trades a feeder bus for a long walk.

## Service Filtering

RAPTOR is calendar-aware. For each query date:

- The active services are determined from `calendar.txt` (day-of-week rules + date ranges)
- Exceptions from `calendar_dates.txt` are applied (additions and removals)
- Only trips belonging to active services are considered during the scan

A query before `routing.service_day_start` (04:00) is answered on the **previous** day's services, shifted by 24 hours, because GTFS files night runs under the day they started (`25:30:00`).

## Real-Time Overlay

Delays and cancellations from GTFS-Realtime feeds are applied at query time, never by rebuilding the index. Each refresh resolves the feed against the schedule into a `RealtimeIndex` keyed by `(pattern_idx, trip_idx)`, and every search takes one snapshot of it:

- arrivals and departures are read through the overlay, so a delayed trip is compared by its real times;
- a cancelled trip, or one that skips a call (it no longer matches the pattern's stop sequence), is not boarded;
- the boarding bounds are widened by the pattern's largest published delay, otherwise a delayed vehicle whose *scheduled* departure is too early would be missed.

## Disruptions

Disruptions authored in the back office are resolved, for the instant of the query, into stop and pattern indices (`DisruptionIndex`):

| Scope | Effect on the search |
|-------|----------------------|
| **Stop** (blocking) | The stop — and its whole station — is neutralized: no boarding, no alighting, no transfer, not even as an origin; vehicles still run through it |
| **Line** (blocking) | Every pattern of the line is added to the excluded patterns for all iterations |
| **Line section** (blocking) | Rides between the section's endpoints are cut, in both directions; the rest of the line stays usable |
| Any scope, `Info` severity | Nothing is removed; the journeys touching it are annotated |

When a blocking disruption is in force, a second, undisrupted RAPTOR pass recovers the fastest journey the traveller would have taken. If it is genuinely explained by a blocking disruption and differs from the journeys found, it is returned with `status: "blocked"` and the disruptions involved, and never receives a quality tag.

## Fuzzy Stop Search

The autocomplete endpoint uses a ranked fuzzy search with French diacritics normalization:

1. **Exact match** (highest priority)
2. **Prefix match** (stop name starts with query)
3. **Word-prefix match** (any word in the stop name starts with query)
4. **Substring match** (query appears anywhere in the stop name)

Names and queries go through `shared::text::normalize` — lowercased, French diacritics stripped, hyphens and apostrophes turned into spaces — so "chatelet" matches "Châtelet" and "saint lazare" matches "Saint-Lazare". BAN address search uses the same normalization.

## Key Optimizations

- **Exact binary search** in `find_earliest_trip`, bounded by per-position departure offsets
- **FIFO boarding pruning** on patterns whose trips never overtake
- **Target and `max_duration` pruning** in `raptor_query_bounded`
- **Search buffers pooled per thread**: the `rounds × stops` tables are handed back when a `RaptorResult` is dropped and reused by the next query on that thread, reset only where the previous query wrote (or by a plain fill past `DENSE_RESET_RATIO`)
- **Searches off the async executor**: the journey handler runs its RAPTOR passes through `web::block`, so a slow search never stalls the other requests of its worker
- **FxHashMap** for stop index and calendar exceptions (faster than default HashMap)
- **Pareto-optimal exploitation** — all Pareto-optimal journeys from a single RAPTOR run are used before re-running
- **Cache persistence** — serialized index with fingerprint-based invalidation
