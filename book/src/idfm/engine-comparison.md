# Glove vs Hove

How good are Glove's journeys? Speed benchmarks ([Performance](../operations/performance.md)) say nothing about it. This page compares Glove with **Hove**, the Navitia engine behind Ile-de-France Mobilités' trip planner, on random addresses across the region. It also describes what the comparison revealed and what was fixed as a result.

## Protocol

| | |
|---|---|
| Origins / destinations | 100 pairs of random addresses from the BAN (`data/ban/adresses-*.csv`), 2 to 40 km apart as the crow flies. Uniform over addresses, hence weighted by where people live |
| Departure | A random minute between 07:00 and 20:00 on Tuesday 2026-10-06 |
| Same request to both | `from=lon;lat`, `to=lon;lat`, `datetime` |
| Schedules | Theoretical on both sides: Glove without real-time, Hove with `data_freshness=base_schedule` |
| Hove access | [PRIM — Navitia API](https://prim.iledefrance-mobilites.fr/fr/apis/idfm-navitia-general-v2), token in the `apiKey` header |
| Journey compared | For each engine, the one arriving **earliest** (ties: fewer transfers, then later departure). Walk-only answers are counted apart |
| Verdict | `tie` when arrivals are within 2 minutes; otherwise `glove_faster` / `hove_faster`; `glove_only` / `hove_only` when one engine finds nothing |
| Glove build | Release, local Valhalla, default `config.yaml` (`prefer_rail: true`) |

Glove's response format mirrors Navitia's (`journeys[].sections[]`), so both engines are read by the same code, and transfers are recounted the same way for both (public-transport sections − 1).

### Plausibility checks

The earliest arrival only means something if the journey can actually be taken. The tool flags a journey as **implausible** when:

- it leaves the origin more than 30 s **before** the requested time;
- one of its vehicle changes takes less than 60 s — a negative gap means boarding a vehicle that has already left.

## Running it

```bash
bin/start.sh                     # Glove on :8080 (or --glove-url)
export PRIM_API_KEY=...          # PRIM token — keep it out of files
python3 scripts/compare_engines.py --pairs 100 --seed 42
```

| Option | Default | |
|---|---|---|
| `--pairs`, `--seed` | 50, 42 | Same seed = same pairs and times, to compare two versions of Glove |
| `--min-km`, `--max-km` | 2, 40 | Crow-fly distance range of a pair |
| `--date`, `--hours` | next Tuesday, 7 20 | Day and hour window of the random departures |
| `--datetime` | — | Fixed departure (`YYYYMMDDTHHMMSS`) instead |
| `--engines` | `glove hove` | The first one is the reference |
| `--glove-url` | `http://localhost:8080` | `--insecure` for Caddy's local CA on `https://api.glove` |
| `--hove-rate` | 2 | Max Hove requests per second (PRIM quota) |
| `--count`, `--hove-freshness` | 5, `base_schedule` | Navitia parameters |

Results go to `results/compare-<timestamp>/` (git-ignored):

- `pairs.csv` — one row per pair, both engines side by side: outcome, departure, arrival, duration, transfers, walk, lines, shortest connection, plausibility issues, verdict;
- `raw.jsonl` — one line per request, with every section of the selected journey;
- the summary printed at the end, including an `implaus` column per engine.

Adding an engine with a Navitia-shaped API is one entry in `build_engines`.

## Results

Six campaigns on the **same 100 pairs** (seed 42), each after one fix (latencies as printed by the tool):

| Campaign | Solved Glove / Hove | Implausible (Glove) | Glove faster / tie / Hove faster | Only one solves | Median arrival gap | Connections (p10) | Glove p50 / p95 |
|---|---|---|---|---|---|---|---|
| Initial | 89 / 93 | **48** | 67 / 17 / 5 | Hove 4 | Glove +6.7 min | 0.7 min | 549 / 1142 ms¹ |
| Alight before boarding | 89 / 93 | 46 | 65 / 19 / 5 | Hove 4 | +6.3 min | 1.0 min | 192 / 432 ms |
| Change time at one stop (120 s) | 89 / 93 | 38 | 60 / 22 / 7 | Hove 4 | +4.9 min | 3.0 min | 183 / 366 ms |
| Access walks timed by Valhalla | 89 / 93 | 0 | 50 / 29 / 10 | Hove 4 | +2.3 min | 3.3 min | 396 / 1045 ms |
| Matrix capped at 30 stops | 89 / 93 | 0 | 45 / 39 / 5 | Hove 4 | +2.0 min | 3.5 min | 257 / 527 ms |
| **Rail change 300 s + fallback radius** | **95** / 93 | **0** | **40 / 43 / 10** | **Glove 2** | **+1.6 min** | **5.5 min** | **263 / 444 ms** |

¹ Debug build — the first campaign ran against a development instance. All later Glove figures are from release builds. Hove's latency (p50 658–733 ms) includes the network round trip to PRIM, so the two are not directly comparable.

For reference, in the last campaign 10 % of Hove's connections take under 5.2 minutes and its journeys average 25.7 minutes of walking, against 29.6 for Glove.

## What the comparison revealed

Glove's initial lead looked convincing — two journeys out of three arriving earlier. It came almost entirely from journeys nobody could take.

### 1. Boarding a vehicle that had already left

In `scan_pattern`, RAPTOR boarded a trip at a stop **and then** recorded that same trip's arrival at that same stop. A train arrives before it departs (the dwell), so the traveller "gained" the dwell and could catch a vehicle that had just left. On pair 61, RAPTOR reached Gare du Nord at 19:59:10 on the RER D. It then "rode" the RER B from Gare du Nord to Gare du Nord, arriving at 19:58:00. That was enough to board the RER B leaving at 19:59:00. `sanitize_sections` then dropped the zero-length section, which hid the cause.

**Fix:** alight before boarding, the reference RAPTOR order. See [RAPTOR — Within each round](../architecture/raptor.md#within-each-round).

### 2. Free changes inside a station

RAPTOR changed vehicles at a stop with **zero** time whenever two lines shared it. IDFM merges every platform of a station into one stop (`monomodalStopPlace`), and `transfers.txt` gives it no time of its own. As a result, RER D → A → B → C through Gare de Lyon, Châtelet and Saint-Michel took 40 s, 7 s and 31 s.

**Fix:** re-boarding at the stop just left costs `routing.default_transfer_time` (120 s). Between two trains, RERs or metros it costs `routing.rail_change_time` (300 s), in line with the 4–8 minutes IDFM gives for changes between distinct stops. A run split into two trip ids on the same route continues free.

### 3. Leaving before the requested time

RAPTOR picked the first stop on a **straight-line** walk at 5 km/h. The walk drawn afterwards came from Valhalla and was a third to a half longer. A stop 1.6 km away as the crow flies was a 29-minute walk, not 19. RAPTOR therefore boarded trains the traveller could not reach, and the journey was shifted back to leave up to 16 minutes before the requested time — on 39 of the 100 pairs.

**Fix:** a Valhalla pedestrian matrix times the walk to the 30 nearest candidate stops before RAPTOR runs (`routing.walk_matrix_stops`). The others are scaled by the largest detour measured. Using the median detour instead let four far RER stations look too close, and four journeys left early again.

### 4. Rural addresses and long walks to the station

Four pairs solved by Hove only had their nearest useful stop 25–31 minutes' walk away, beyond the 1500 m radius. On the five pairs Hove won, `prefer_rail` made Glove walk 25–46 minutes to a station where Hove took a feeder bus.

**Fix:** a second search within `routing.fallback_stop_distance` (2500 m) when an address finds nothing. `prefer_rail` now only applies with a rail stop within `routing.prefer_rail_max_walk` (600 s) of both ends.

## Assessment

After these fixes, the two engines are **on a par**:

- **No implausible journey** on either side.
- **Glove answers more pairs**: 95 against 93. It solves the four rural pairs Hove used to win alone, and two that Hove does not solve.
- **Same line sequence** as Hove in 47 of the 93 pairs both solve; 43 ties within 2 minutes.
- **A small remaining lead of 1.6 minutes** (median), with connections now as conservative as Hove's (10th percentile 5.5 vs 5.2 min).

Part of that lead comes from the criterion. Only the earliest arrival is compared, and Glove accepts more walking to arrive sooner (29.6 vs 25.7 minutes on average). Pair 58 arrives earlier with an hour of walking, where Hove takes a feeder bus. Judging journeys on walking and transfers as well, not arrival alone, would sharpen the comparison.

## Limits

- **100 pairs**: proportions are good to about ±10 points.
- **One weekday**, 07:00–20:00. Weekends, evenings and nights are not covered.
- **Theoretical schedules only**: real-time quality is not measured.
- **No ground truth**: "arrives earlier" is only better if the journey is realistic. The plausibility checks catch the impossible, not the merely optimistic.
- **Approximate walking time**: the sum of street and transfer sections, which the two engines split differently.
- **One configuration**: Glove ran with the defaults of `config.yaml.sample`. Change times, radii and `prefer_rail` all move the results.
