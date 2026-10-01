#!/usr/bin/env python3
"""
Journey planner comparison — Glove vs Hove (Navitia via PRIM) and more.

Draws random origin/destination pairs from the BAN address files
(data/ban/adresses-*.csv), asks every engine for public transport journeys
at the same departure time, and compares the answers: earliest arrival,
duration, transfers, walking time, latency and failures.

Usage:
    export PRIM_API_KEY=...            # sent as the `apiKey` header to PRIM
    python3 scripts/compare_engines.py --pairs 100 --seed 42
    python3 scripts/compare_engines.py --engines glove --glove-url https://api.glove --insecure

Outputs (in --out, default results/compare-<timestamp>/):
    raw.jsonl     one line per (pair, engine): request, metrics, error
    pairs.csv     one line per pair, both engines side by side
    stdout        aggregate summary

Stdlib only. Engines share a single normalizer because Glove's response
mirrors the Navitia journey format (journeys[].sections[], ISO basic dates).
"""

import argparse
import csv
import datetime as dt
import glob
import json
import math
import os
import random
import ssl
import statistics
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field

PRIM_NAVITIA_URL = "https://prim.iledefrance-mobilites.fr/marketplace/v2/navitia"
NAVITIA_DATE_FORMAT = "%Y%m%dT%H%M%S"
REQUEST_TIMEOUT_SECS = 60
# Arrivals closer than this are a tie: schedules are minute-grained and the
# engines model first/last-mile walking differently.
TIE_TOLERANCE_SECS = 120
WALK_SECTION_TYPES = {"street_network", "transfer", "crow_fly"}
PT_SECTION_TYPES = {"public_transport", "on_demand_transport"}
# Below this, a vehicle change is not something a traveller can make (and a
# negative gap means boarding a vehicle that already left). Flagged, not dropped.
MIN_PLAUSIBLE_CONNECTION_SECS = 60
# Leaving the origin earlier than the requested time, beyond rounding.
EARLY_DEPARTURE_TOLERANCE_SECS = 30


# ---------------------------------------------------------------------------
# Engines
# ---------------------------------------------------------------------------

@dataclass
class Engine:
    """A journey planner reachable through a Navitia-shaped HTTP API."""
    name: str
    journeys_url: str
    headers: dict = field(default_factory=dict)
    extra_params: dict = field(default_factory=dict)
    min_interval_secs: float = 0.0   # client-side throttle (provider quotas)
    _last_call: float = 0.0

    def build_url(self, origin, destination, when):
        params = {
            "from": f"{origin.lon};{origin.lat}",
            "to": f"{destination.lon};{destination.lat}",
            "datetime": when.strftime(NAVITIA_DATE_FORMAT),
            **self.extra_params,
        }
        return f"{self.journeys_url}?{urllib.parse.urlencode(params, safe=';')}"

    def throttle(self):
        wait = self._last_call + self.min_interval_secs - time.monotonic()
        if wait > 0:
            time.sleep(wait)
        self._last_call = time.monotonic()


def build_engines(args):
    """Instantiate the engines selected on the command line."""
    available = {}
    available["glove"] = lambda: Engine(
        name="glove",
        journeys_url=f"{args.glove_url.rstrip('/')}/api/journeys/public_transport",
    )

    def hove():
        api_key = os.environ.get("PRIM_API_KEY", "")
        if not api_key:
            sys.exit("PRIM_API_KEY is not set (PRIM token, sent as the apiKey header)")
        return Engine(
            name="hove",
            journeys_url=f"{args.hove_url.rstrip('/')}/journeys",
            headers={"apiKey": api_key},
            # Theoretical timetable on both sides: Glove routes on the GTFS
            # without real-time unless configured, so compare like with like.
            extra_params={"data_freshness": args.hove_freshness, "count": args.count},
            min_interval_secs=1.0 / args.hove_rate if args.hove_rate > 0 else 0.0,
        )
    available["hove"] = hove

    unknown = set(args.engines) - available.keys()
    if unknown:
        sys.exit(f"Unknown engine(s): {', '.join(sorted(unknown))} "
                 f"(available: {', '.join(available)})")
    return [available[name]() for name in args.engines]


# ---------------------------------------------------------------------------
# BAN sampling
# ---------------------------------------------------------------------------

@dataclass
class Address:
    id: str
    label: str
    lon: float
    lat: float


def address_from_row(row):
    number = f"{row['numero']}{row.get('rep') or ''}"
    label = f"{number} {row['nom_voie']}, {row['code_postal']} {row['nom_commune']}"
    return Address(row["id"], label, float(row["lon"]), float(row["lat"]))


def sample_addresses(ban_dir, count, rng):
    """Reservoir-sample `count` addresses uniformly across all BAN files.

    Uniform over addresses, hence weighted by address density — which is
    roughly where people actually live and travel from.
    """
    files = sorted(glob.glob(os.path.join(ban_dir, "adresses-*.csv")))
    if not files:
        sys.exit(f"No BAN file in {ban_dir} (run bin/download.sh)")
    reservoir, seen = [], 0
    for path in files:
        with open(path, encoding="utf-8", newline="") as f:
            for row in csv.DictReader(f, delimiter=";"):
                if not row.get("lon") or not row.get("lat"):
                    continue
                seen += 1
                if len(reservoir) < count:
                    reservoir.append(row)
                else:
                    slot = rng.randrange(seen)
                    if slot < count:
                        reservoir[slot] = row
    rng.shuffle(reservoir)
    return [address_from_row(row) for row in reservoir]


def haversine_km(a, b):
    lat1, lat2 = math.radians(a.lat), math.radians(b.lat)
    dlat, dlon = lat2 - lat1, math.radians(b.lon - a.lon)
    h = math.sin(dlat / 2) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2) ** 2
    return 2 * 6371.0 * math.asin(math.sqrt(h))


def draw_pairs(addresses, args, rng):
    """Pair addresses whose crow-fly distance lies in [min_km, max_km]."""
    pairs, pool = [], list(addresses)
    while len(pairs) < args.pairs and len(pool) >= 2:
        origin = pool.pop()
        for i, candidate in enumerate(pool):
            if args.min_km <= haversine_km(origin, candidate) <= args.max_km:
                pairs.append((origin, pool.pop(i), draw_departure(args, rng)))
                break
    if len(pairs) < args.pairs:
        print(f"warning: only {len(pairs)} pairs within "
              f"[{args.min_km}, {args.max_km}] km", file=sys.stderr)
    return pairs


def draw_departure(args, rng):
    """Fixed --datetime, or a random minute of --date within --hours."""
    if args.datetime:
        return dt.datetime.strptime(args.datetime, NAVITIA_DATE_FORMAT)
    start_h, end_h = args.hours
    minute = rng.randrange(start_h * 60, end_h * 60)
    return dt.datetime.combine(args.date, dt.time(minute // 60, minute % 60))


def next_weekday(weekday=1):
    """Next given weekday (default Tuesday): a plain working day, never today."""
    today = dt.date.today()
    return today + dt.timedelta(days=(weekday - today.weekday() - 1) % 7 + 1)


# ---------------------------------------------------------------------------
# Querying and normalization
# ---------------------------------------------------------------------------

def call_engine(engine, url, ssl_context):
    """GET a journeys URL. Returns (http_status, json_body | None, latency_ms, error)."""
    engine.throttle()
    request = urllib.request.Request(url, headers={"Accept": "application/json", **engine.headers})
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(request, timeout=REQUEST_TIMEOUT_SECS,
                                    context=ssl_context) as resp:
            body = resp.read()
            status = resp.status
    except urllib.error.HTTPError as e:
        body, status = e.read(), e.code
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        return None, None, (time.perf_counter() - started) * 1000, f"transport: {e}"
    latency_ms = (time.perf_counter() - started) * 1000
    try:
        return status, json.loads(body), latency_ms, None
    except json.JSONDecodeError:
        return status, None, latency_ms, f"invalid JSON (HTTP {status})"


def parse_date(value):
    return dt.datetime.strptime(value, NAVITIA_DATE_FORMAT)


def has_public_transport(journey):
    return any(s.get("type") in PT_SECTION_TYPES for s in journey.get("sections", []))


def summarize_journey(journey):
    """Reduce a Navitia-shaped journey to the comparable metrics."""
    sections = journey.get("sections", [])
    pt = [s for s in sections if s.get("type") in PT_SECTION_TYPES]
    lines = []
    for s in pt:
        info = s.get("display_informations") or {}
        lines.append(f"{info.get('commercial_mode', '?')} {info.get('label', '?')}".strip())
    return {
        "departure": journey["departure_date_time"],
        "arrival": journey["arrival_date_time"],
        "duration": journey.get("duration"),
        # Navitia's nb_transfers counts vehicle changes; recompute so both
        # engines are measured the same way.
        "transfers": max(len(pt) - 1, 0),
        "walk_secs": sum(s.get("duration", 0) for s in sections
                         if s.get("type") in WALK_SECTION_TYPES),
        "lines": lines,
        "min_connection_secs": min_connection_secs(pt),
        # Kept in raw.jsonl only: enough to audit connections and access legs.
        "legs": [summarize_section(s) for s in sections],
    }


def min_connection_secs(pt_sections):
    """Shortest gap between alighting one vehicle and boarding the next."""
    gaps = [(parse_date(b["departure_date_time"]) - parse_date(a["arrival_date_time"])).total_seconds()
            for a, b in zip(pt_sections, pt_sections[1:])]
    return min(gaps) if gaps else None


def plausibility_issues(best, when):
    """Why the selected journey could not be taken as returned (empty = plausible)."""
    issues = []
    early = (when - parse_date(best["departure"])).total_seconds()
    if early > EARLY_DEPARTURE_TOLERANCE_SECS:
        issues.append(f"departs_{int(early)}s_before_query")
    gap = best["min_connection_secs"]
    if gap is not None and gap < MIN_PLAUSIBLE_CONNECTION_SECS:
        issues.append(f"connection_{int(gap)}s")
    return issues


def summarize_section(section):
    info = section.get("display_informations") or {}
    return {
        "type": section.get("type"),
        "departure": section.get("departure_date_time"),
        "arrival": section.get("arrival_date_time"),
        "duration": section.get("duration"),
        "label": f"{info.get('commercial_mode', '')} {info.get('label', '')}".strip(),
        "from": (section.get("from") or {}).get("name", ""),
        "to": (section.get("to") or {}).get("name", ""),
    }


def evaluate(engine, origin, destination, when, ssl_context):
    """Query one engine for one pair and return a flat result record."""
    url = engine.build_url(origin, destination, when)
    status, body, latency_ms, error = call_engine(engine, url, ssl_context)
    record = {"engine": engine.name, "http_status": status,
              "latency_ms": round(latency_ms, 1), "outcome": "ok", "error": error}
    if error:
        record["outcome"] = "error"
        return record

    journeys = [j for j in (body or {}).get("journeys", []) if has_public_transport(j)]
    walk_only = len((body or {}).get("journeys", [])) - len(journeys)
    record["nb_journeys"], record["nb_walk_only"] = len(journeys), walk_only
    if not journeys:
        api_error = (body or {}).get("error") or {}
        record["outcome"] = "walk_only" if walk_only else "no_solution"
        record["error"] = api_error.get("id") or api_error.get("message") or (
            None if status == 200 else f"HTTP {status}")
        return record

    # Pareto quality on (arrival, transfers) — "best" is the earliest arrival,
    # ties broken by fewer transfers then later departure.
    summaries = [summarize_journey(j) for j in journeys]
    best = min(summaries, key=lambda s: (s["arrival"], s["transfers"],
                                         _negated(s["departure"])))
    record["best"] = best
    record["issues"] = plausibility_issues(best, when)
    record["min_transfers"] = min(s["transfers"] for s in summaries)
    return record


def _negated(departure):
    return -parse_date(departure).timestamp()


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------

def compare_pair(results, reference, challenger):
    """Arrival delta (challenger - reference, seconds) and verdict for a pair."""
    ref, other = results.get(reference), results.get(challenger)
    if not ref or not other:
        return None, "n/a"
    ref_ok, other_ok = "best" in ref, "best" in other
    if not ref_ok and not other_ok:
        return None, "both_fail"
    if not other_ok:
        return None, f"{reference}_only"
    if not ref_ok:
        return None, f"{challenger}_only"
    delta = (parse_date(other["best"]["arrival"])
             - parse_date(ref["best"]["arrival"])).total_seconds()
    if abs(delta) <= TIE_TOLERANCE_SECS:
        return delta, "tie"
    return delta, (reference if delta > 0 else challenger) + "_faster"


def percentile(values, q):
    if not values:
        return float("nan")
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(round(q * (len(ordered) - 1))))]


def print_summary(engines, rows, records):
    names = [e.name for e in engines]
    print(f"\n=== {len(rows)} pairs ===\n")
    print(f"{'engine':<8} {'ok':>5} {'no_sol':>7} {'walk':>5} {'error':>6} "
          f"{'p50 ms':>8} {'p95 ms':>8} {'avg dur':>8} {'avg tr':>7} {'avg walk':>9} {'implaus':>8}")
    for name in names:
        mine = [r for r in records if r["engine"] == name]
        ok = [r for r in mine if "best" in r]
        lat = [r["latency_ms"] for r in mine if r["outcome"] != "error"]
        count = lambda outcome: sum(r["outcome"] == outcome for r in mine)
        avg = lambda key: statistics.mean(r["best"][key] for r in ok) if ok else float("nan")
        print(f"{name:<8} {len(ok):>5} {count('no_solution'):>7} {count('walk_only'):>5} "
              f"{count('error'):>6} {percentile(lat, .5):>8.0f} {percentile(lat, .95):>8.0f} "
              f"{avg('duration') / 60:>7.1f}m {avg('transfers'):>7.2f} {avg('walk_secs') / 60:>8.1f}m "
              f"{sum(bool(r['issues']) for r in ok):>8}")

    if len(names) < 2:
        return
    reference, challenger = names[0], names[1]
    verdicts = {}
    for row in rows:
        verdicts[row["verdict"]] = verdicts.get(row["verdict"], 0) + 1
    print(f"\nEarliest arrival, {challenger} vs {reference} (tie = ±{TIE_TOLERANCE_SECS}s):")
    for verdict, n in sorted(verdicts.items(), key=lambda kv: -kv[1]):
        print(f"  {verdict:<20} {n:>5}  ({100 * n / len(rows):.0f}%)")
    deltas = [row["arrival_delta_secs"] for row in rows if row["arrival_delta_secs"] is not None]
    if deltas:
        print(f"\nArrival delta {challenger} - {reference} (min): "
              f"median {statistics.median(deltas) / 60:+.1f}, "
              f"p10 {percentile(deltas, .1) / 60:+.1f}, p90 {percentile(deltas, .9) / 60:+.1f}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def parse_args():
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[1],
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--engines", nargs="+", default=["glove", "hove"],
                   help="engines to compare; the first is the reference (default: glove hove)")
    p.add_argument("--pairs", type=int, default=50)
    p.add_argument("--seed", type=int, default=42, help="same seed = same pairs and times")
    p.add_argument("--ban-dir", default="data/ban")
    p.add_argument("--min-km", type=float, default=2.0, help="min crow-fly O/D distance")
    p.add_argument("--max-km", type=float, default=40.0, help="max crow-fly O/D distance")
    p.add_argument("--date", type=dt.date.fromisoformat, default=next_weekday(),
                   help="departure day, YYYY-MM-DD (default: next Tuesday)")
    p.add_argument("--hours", type=int, nargs=2, default=[7, 20], metavar=("FROM", "TO"),
                   help="random departure hour window (default: 7 20)")
    p.add_argument("--datetime", help="fixed departure YYYYMMDDTHHMMSS (overrides --date/--hours)")
    p.add_argument("--count", type=int, default=5, help="journeys requested from Hove")
    p.add_argument("--glove-url", default="http://localhost:8080")
    p.add_argument("--hove-url", default=PRIM_NAVITIA_URL)
    p.add_argument("--hove-freshness", default="base_schedule",
                   choices=["base_schedule", "adapted_schedule", "realtime"])
    p.add_argument("--hove-rate", type=float, default=2.0,
                   help="max Hove requests per second (PRIM quota); 0 = unthrottled")
    p.add_argument("--insecure", action="store_true",
                   help="skip TLS verification (Caddy's local CA on https://api.glove)")
    p.add_argument("--out", help="output directory (default: results/compare-<timestamp>)")
    args = p.parse_args()
    if args.hours[0] >= args.hours[1] or not (0 <= args.hours[0] and args.hours[1] <= 24):
        p.error("--hours expects FROM < TO within 0..24")
    return args


def main():
    args = parse_args()
    engines = build_engines(args)
    ssl_context = ssl._create_unverified_context() if args.insecure else None
    rng = random.Random(args.seed)

    print(f"Sampling BAN addresses from {args.ban_dir} ...", file=sys.stderr)
    # Oversample: the distance filter rejects some candidates.
    addresses = sample_addresses(args.ban_dir, args.pairs * 4, rng)
    pairs = draw_pairs(addresses, args, rng)

    out_dir = args.out or os.path.join(
        "results", "compare-" + dt.datetime.now().strftime("%Y%m%d-%H%M%S"))
    os.makedirs(out_dir, exist_ok=True)
    names = [e.name for e in engines]
    rows, records = [], []

    with open(os.path.join(out_dir, "raw.jsonl"), "w", encoding="utf-8") as raw:
        for index, (origin, destination, when) in enumerate(pairs, 1):
            results = {}
            for engine in engines:
                record = evaluate(engine, origin, destination, when, ssl_context)
                record.update(pair=index, datetime=when.strftime(NAVITIA_DATE_FORMAT),
                              origin=vars(origin), destination=vars(destination))
                raw.write(json.dumps(record, ensure_ascii=False) + "\n")
                records.append(record)
                results[engine.name] = record
            delta, verdict = (compare_pair(results, names[0], names[1])
                              if len(names) > 1 else (None, "n/a"))
            rows.append(flatten_row(index, origin, destination, when, results,
                                    names, delta, verdict))
            print(f"[{index}/{len(pairs)}] {haversine_km(origin, destination):5.1f} km "
                  f"{when:%H:%M}  " + "  ".join(f"{n}={results[n]['outcome']}" for n in names)
                  + f"  -> {verdict}", file=sys.stderr)

    write_pairs_csv(os.path.join(out_dir, "pairs.csv"), rows)
    print_summary(engines, rows, records)
    print(f"\nResults written to {out_dir}/")


def flatten_row(index, origin, destination, when, results, names, delta, verdict):
    row = {
        "pair": index, "datetime": when.strftime(NAVITIA_DATE_FORMAT),
        "distance_km": round(haversine_km(origin, destination), 2),
        "from": origin.label, "from_coord": f"{origin.lon};{origin.lat}",
        "to": destination.label, "to_coord": f"{destination.lon};{destination.lat}",
        "verdict": verdict, "arrival_delta_secs": delta,
    }
    for name in names:
        r, best = results[name], results[name].get("best") or {}
        row.update({
            f"{name}_outcome": r["outcome"], f"{name}_error": r.get("error") or "",
            f"{name}_latency_ms": r["latency_ms"],
            f"{name}_departure": best.get("departure", ""),
            f"{name}_arrival": best.get("arrival", ""),
            f"{name}_duration": best.get("duration", ""),
            f"{name}_transfers": best.get("transfers", ""),
            f"{name}_walk_secs": best.get("walk_secs", ""),
            f"{name}_lines": " > ".join(best.get("lines", [])),
            f"{name}_min_connection_secs": best.get("min_connection_secs", ""),
            f"{name}_issues": " ".join(r.get("issues", [])),
        })
    return row


def write_pairs_csv(path, rows):
    if not rows:
        return
    with open(path, "w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
