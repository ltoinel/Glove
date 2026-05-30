#!/usr/bin/env python3
"""
Glove — GTFS pathways vs. Valhalla indoor walking time

For every entry in GTFS `pathways.txt` that has a `traversal_time`, this script
asks Valhalla for a pedestrian route between the two pathway nodes (using the
same indoor-friendly costing as Glove's transfer enrichment: no step/elevator
penalty, tunnels allowed) and compares the two durations.

A pathway is counted as having indoor data **present** only when Valhalla's
route actually uses an indoor feature — i.e. it contains an elevator / stairs /
escalator / building enter-exit maneuver (Valhalla maneuver types 39-43). Other
pathways are routed over the street network and are reported separately.

Usage:
    python3 scripts/pathway_valhalla_diff.py \
        [--gtfs data/gtfs] [--valhalla http://localhost:8002] \
        [--concurrency 8] [--top 25] [--output scripts/pathway_diff.csv]

Requires Valhalla running with the Ile-de-France OSM data loaded.
"""

import argparse
import csv
import json
import os
import statistics
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed

# Valhalla maneuver types that prove the route went through modelled indoor
# infrastructure (see portal maneuverIcon / INDOOR_MANEUVER_TYPES).
INDOOR_MANEUVER_TYPES = {39, 40, 41, 42, 43}  # elevator, steps, escalator, enter, exit


def load_stops(gtfs_dir):
    """stop_id -> (lon, lat) for stops that have coordinates."""
    stops = {}
    with open(os.path.join(gtfs_dir, "stops.txt"), newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            lon, lat = r.get("stop_lon"), r.get("stop_lat")
            if lon and lat:
                try:
                    stops[r["stop_id"]] = (float(lon), float(lat))
                except ValueError:
                    pass
    return stops


def load_pathways(gtfs_dir, stops):
    """List of pathways that have a traversal_time and resolvable endpoints."""
    out = []
    with open(os.path.join(gtfs_dir, "pathways.txt"), newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            tt = r.get("traversal_time")
            a = stops.get(r.get("from_stop_id"))
            b = stops.get(r.get("to_stop_id"))
            if not tt or a is None or b is None:
                continue
            try:
                tt = int(float(tt))
            except ValueError:
                continue
            out.append({
                "id": r.get("pathway_id", ""),
                "from": r["from_stop_id"], "to": r["to_stop_id"],
                "from_coord": a, "to_coord": b,
                "length": float(r["length"]) if r.get("length") else None,
                "traversal_time": tt,
            })
    return out


def valhalla_route(base, a, b):
    """Return (time_s, indoor_bool, n_maneuvers) or None on failure."""
    body = {
        "locations": [
            {"lon": a[0], "lat": a[1]},
            {"lon": b[0], "lat": b[1]},
        ],
        "costing": "pedestrian",
        "costing_options": {
            "pedestrian": {"step_penalty": 0, "elevator_penalty": 0, "use_tunnels": 1.0}
        },
        "directions_options": {"units": "kilometers"},
    }
    req = urllib.request.Request(
        f"{base}/route",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read())
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, OSError):
        return None
    legs = data.get("trip", {}).get("legs", [])
    if not legs:
        return None
    maneuvers = legs[0].get("maneuvers", [])
    indoor = any(m.get("type") in INDOOR_MANEUVER_TYPES for m in maneuvers)
    time_s = int(data["trip"]["summary"]["time"])
    return (time_s, indoor, len(maneuvers))


def main():
    ap = argparse.ArgumentParser(description="GTFS pathways vs Valhalla indoor time")
    ap.add_argument("--gtfs", default="data/gtfs")
    ap.add_argument("--valhalla", default="http://localhost:8002")
    ap.add_argument("--concurrency", type=int, default=8)
    ap.add_argument("--top", type=int, default=25, help="largest gaps to print")
    ap.add_argument("--output", default="scripts/pathway_diff.csv")
    args = ap.parse_args()

    stops = load_stops(args.gtfs)
    pathways = load_pathways(args.gtfs, stops)
    print(f"Loaded {len(pathways)} pathways with traversal_time and coordinates.\n")

    rows = []
    routed = 0
    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = {
            pool.submit(valhalla_route, args.valhalla, p["from_coord"], p["to_coord"]): p
            for p in pathways
        }
        done = 0
        for fut in as_completed(futures):
            p = futures[fut]
            res = fut.result()
            done += 1
            if done % 500 == 0:
                print(f"  ...{done}/{len(pathways)}")
            if res is None:
                continue
            routed += 1
            time_s, indoor, n_man = res
            rows.append({
                "pathway_id": p["id"], "from": p["from"], "to": p["to"],
                "length_m": p["length"], "gtfs_traversal_s": p["traversal_time"],
                "valhalla_s": time_s, "diff_s": time_s - p["traversal_time"],
                "indoor": indoor, "n_maneuvers": n_man,
            })

    indoor_rows = [r for r in rows if r["indoor"]]
    diffs = [r["diff_s"] for r in indoor_rows]

    print(f"\n{'='*64}")
    print(f"  Pathways compared (Valhalla routed): {routed}/{len(pathways)}")
    print(f"  With indoor data present (indoor maneuvers): {len(indoor_rows)}")
    if diffs:
        abs_diffs = [abs(d) for d in diffs]
        slower = sum(1 for d in diffs if d > 0)
        faster = sum(1 for d in diffs if d < 0)
        print(f"  (Valhalla − GTFS, indoor-present pathways)")
        print(f"    mean diff   : {statistics.mean(diffs):+8.1f} s")
        print(f"    median diff : {statistics.median(diffs):+8.1f} s")
        print(f"    mean |diff| : {statistics.mean(abs_diffs):8.1f} s")
        print(f"    Valhalla slower than GTFS: {slower}  | faster: {faster}")
    print(f"{'='*64}\n")

    # Largest discrepancies (indoor-present, by absolute gap)
    indoor_rows.sort(key=lambda r: abs(r["diff_s"]), reverse=True)
    print(f"  Top {args.top} indoor gaps (|Valhalla − GTFS|):")
    print(f"  {'GTFS':>6} {'Valh':>6} {'diff':>7} {'len':>6}  from → to")
    for r in indoor_rows[: args.top]:
        ln = f"{r['length_m']:.0f}" if r["length_m"] else "?"
        print(f"  {r['gtfs_traversal_s']:>6} {r['valhalla_s']:>6} {r['diff_s']:>+7} "
              f"{ln:>6}  {r['from']} → {r['to']}")

    rows.sort(key=lambda r: abs(r["diff_s"]), reverse=True)
    with open(args.output, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()) if rows else
                           ["pathway_id", "from", "to", "length_m",
                            "gtfs_traversal_s", "valhalla_s", "diff_s", "indoor", "n_maneuvers"])
        w.writeheader()
        w.writerows(rows)
    print(f"\n  Full results written to {args.output}")


if __name__ == "__main__":
    main()
