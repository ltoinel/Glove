# Performance

## Benchmarks

The RAPTOR engine holds all GTFS data in memory with optimized data structures, so the core round-based scan is fast (single-digit to low-tens of milliseconds). The **end-to-end** response time of `GET /api/journeys/public_transport` is higher because each request also runs the **iterative diverse search** (RAPTOR is re-run up to `max_journeys` times with pattern exclusion to produce varied alternatives) and **enriches transfers via Valhalla** (one walking-route call per transfer).

Benchmark across 12 origin/destination pairs covering Ile-de-France (10 rounds, single-threaded, weekday morning departure), with the default `config.yaml` (`max_journeys: 5`, `prefer_rail: true`):

![Benchmark](../images/benchmark.png)

| Metric | Value |
|--------|-------|
| Min | 270 ms |
| Avg | 1,425 ms |
| Median | 1,362 ms |
| p95 | 3,363 ms |
| Max | 4,281 ms |

```admonish note
These are **end-to-end** API times (iterative diverse search + Valhalla transfer enrichment), not the bare RAPTOR scan. The query is bounded by **target + max-duration pruning** (`raptor_query_bounded`): the scan stops relaxing stops that can no longer improve the journey to the destination within `max_duration`. This roughly halved suburban routes (e.g. Massy-Verrières → Châtelet). The remaining cost is dominated by the **number of alternatives** (`max_journeys`): central routes that return 5–7 distinct journeys re-run RAPTOR several times and are the slowest. Lowering `max_journeys` reduces response time further.
```

## Running Benchmarks

```bash
python3 scripts/benchmark.py --rounds 10 --concurrency 1 --datetime 20260529T083000
```

The benchmark script:
1. Sends requests to 12 representative origin/destination pairs (stop-to-stop)
2. Measures response times across multiple rounds
3. Prints a summary table and generates a chart (`--output`, default `docs/benchmark.png`)

```admonish tip
Pick a `--datetime` that falls inside the loaded GTFS service window (otherwise no journeys are found). For high request rates, set `server.rate_limit: 0` in `config.yaml` so the rate limiter does not reject the burst.
```

## Key Optimizations

### Binary Search in Trip Lookup
The `find_earliest_trip` function uses binary search (O(log n)) to find the first trip departing after a given time within a pattern, instead of linear scan.

### Pre-Allocated Buffers
Label arrays and working buffers are allocated once and reused across RAPTOR rounds, eliminating per-round allocation overhead.

### FxHashMap
Uses `rustc-hash`'s FxHashMap throughout both `GtfsData` and `RaptorData`, replacing all standard library `HashMap` instances. FxHash is significantly faster than the default SipHash for integer and string keys.

### Lock-Free Hot-Reload
ArcSwap provides atomic pointer swaps with zero contention. Readers never block, even during a reload. There is no mutex, no RwLock, and no read-side overhead.

### Target + Max-Duration Pruning
`raptor_query_bounded` tracks an upper bound = `min(departure + max_duration, best arrival at the destination)` and refreshes it after each round. Relaxations (and the transfers/markings they trigger) beyond that bound are skipped, so the scan no longer expands the whole 54,000-stop network toward irrelevant or too-distant stops. This roughly halves long suburban queries while returning identical journeys.

### Early Termination in Diversity Loop
The RAPTOR diversity loop (which re-runs the algorithm with pattern exclusion to find alternative journeys) terminates early when a round produces no new journeys, avoiding unnecessary iterations.

### Arc&lt;WalkLeg&gt; Cache
Walking leg results from Valhalla are wrapped in `Arc<WalkLeg>` and cached, avoiding deep cloning of polyline coordinates when the same walk leg is referenced by multiple journeys.

### Batch Valhalla Calls
Transfer enrichment requests to Valhalla are dispatched in parallel using `futures::join_all`, rather than sequentially, reducing latency for journeys with multiple transfers.

### Pareto-Optimal Exploitation
All Pareto-optimal journeys from a single RAPTOR run are collected before the algorithm is re-run with pattern exclusion. This avoids redundant computation.

### Cache Persistence
The RAPTOR index is serialized to disk with a fingerprint derived from the GTFS data. On restart, if the fingerprint matches, the cached index is loaded in sub-second time instead of rebuilding (10-30 seconds).

### Pattern Grouping
Trips with identical stop sequences share a single pattern. For the Ile-de-France dataset this collapses ~391,000 trips into ~10,000 patterns, cutting the entities the algorithm scans by an order of magnitude.

## Memory Usage

All GTFS data is held in memory. For the Ile-de-France dataset (as reported by `GET /api/status` and `GET /api/metrics`):

| Data | Approximate Size |
|------|-----------------|
| Stops | ~53,700 entries |
| Routes | ~2,000 entries |
| Trips | ~391,000 entries |
| Stop times | ~8,370,000 entries |
| Transfers | ~202,000 entries |
| Patterns | ~10,000 groups |
| Resident memory (RSS) | ~265 MB |
| Virtual memory | ~480 MB |
