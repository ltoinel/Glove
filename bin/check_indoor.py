#!/usr/bin/env python3
"""Check which GTFS transfer pairs have indoor routing data in Valhalla.

Queries Valhalla's pedestrian route API for transfer stop pairs and checks
whether the response contains indoor maneuver types (elevator, stairs,
escalator, enter/exit building).

Produces:
  - A detailed CSV file with all results (one row per transfer pair)
  - A summary CSV grouped by station
  - Console report with statistics

Usage:
    python3 bin/check_indoor.py [--valhalla http://localhost:8002] [--limit 0] [--min-distance 0]
    python3 bin/check_indoor.py --output data/indoor_report.csv --summary data/indoor_summary.csv
"""

import argparse
import csv
import json
import math
import sys
import time
import urllib.request
from collections import defaultdict
from pathlib import Path

INDOOR_TYPES = {
    39: "elevator",
    40: "stairs",
    41: "escalator",
    42: "enter_building",
    43: "exit_building",
}

ALL_MANEUVER_TYPES = {
    0: "none", 1: "start", 2: "start_right", 3: "start_left",
    4: "destination", 5: "destination_right", 6: "destination_left",
    7: "becomes", 8: "continue", 9: "slight_right", 10: "right",
    11: "sharp_right", 12: "uturn_right", 13: "uturn_left",
    14: "sharp_left", 15: "left", 16: "slight_left",
    17: "ramp_straight", 18: "ramp_right", 19: "ramp_left",
    20: "exit_right", 21: "exit_left", 22: "stay_straight",
    23: "stay_right", 24: "stay_left", 25: "merge",
    26: "roundabout_enter", 27: "roundabout_exit",
    28: "ferry_enter", 29: "ferry_exit",
    39: "elevator", 40: "stairs", 41: "escalator",
    42: "enter_building", 43: "exit_building",
}


def haversine(lat1, lon1, lat2, lon2):
    """Distance in meters between two coordinates."""
    R = 6_371_000
    dlat = math.radians(lat2 - lat1)
    dlon = math.radians(lon2 - lon1)
    a = (math.sin(dlat / 2) ** 2
         + math.cos(math.radians(lat1)) * math.cos(math.radians(lat2))
         * math.sin(dlon / 2) ** 2)
    return R * 2 * math.atan2(math.sqrt(a), math.sqrt(1 - a))


def load_stops(gtfs_dir):
    """Load stops from GTFS stops.txt into {stop_id: (lat, lon, name, parent)}."""
    stops = {}
    with open(gtfs_dir / "stops.txt", encoding="utf-8-sig") as f:
        for row in csv.DictReader(f):
            try:
                stops[row["stop_id"]] = (
                    float(row["stop_lat"]),
                    float(row["stop_lon"]),
                    row.get("stop_name", ""),
                    row.get("parent_station", ""),
                )
            except (ValueError, KeyError):
                continue
    return stops


def load_transfers(gtfs_dir, stops, min_distance):
    """Load unique transfer pairs with distance >= min_distance meters."""
    pairs = []
    seen = set()
    with open(gtfs_dir / "transfers.txt", encoding="utf-8-sig") as f:
        for row in csv.DictReader(f):
            from_id = row["from_stop_id"]
            to_id = row["to_stop_id"]
            if from_id == to_id:
                continue
            key = tuple(sorted([from_id, to_id]))
            if key in seen:
                continue
            seen.add(key)
            if from_id not in stops or to_id not in stops:
                continue
            f_lat, f_lon, _, _ = stops[from_id]
            t_lat, t_lon, _, _ = stops[to_id]
            dist = haversine(f_lat, f_lon, t_lat, t_lon)
            if dist >= min_distance:
                pairs.append((from_id, to_id, dist))
    return pairs


def query_valhalla(base_url, from_lat, from_lon, to_lat, to_lon):
    """Query Valhalla pedestrian route. Returns (maneuvers, route_distance, route_duration) or None."""
    body = json.dumps({
        "locations": [
            {"lat": from_lat, "lon": from_lon},
            {"lat": to_lat, "lon": to_lon},
        ],
        "costing": "pedestrian",
        "costing_options": {"pedestrian": {"step_penalty": 30, "elevator_penalty": 60}},
        "directions_options": {"units": "kilometers"},
    }).encode()
    req = urllib.request.Request(
        f"{base_url}/route",
        data=body,
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read())
            trip = data.get("trip", {})
            summary = trip.get("summary", {})
            route_dist = round(summary.get("length", 0) * 1000)
            route_time = round(summary.get("time", 0))
            legs = trip.get("legs", [{}])
            maneuvers = legs[0].get("maneuvers", []) if legs else []
            return (
                [(m["type"], m.get("instruction", "")) for m in maneuvers],
                route_dist,
                route_time,
            )
    except Exception:
        return None


def station_name(stops, stop_id):
    """Resolve to parent station name if available."""
    _, _, name, parent = stops[stop_id]
    if parent and parent in stops:
        _, _, pname, _ = stops[parent]
        if pname:
            return pname
    return name


def main():
    parser = argparse.ArgumentParser(description="Check indoor routing data in Valhalla")
    parser.add_argument("--valhalla", default="http://localhost:8002", help="Valhalla base URL")
    parser.add_argument("--gtfs-dir", default="data/gtfs", help="GTFS data directory")
    parser.add_argument("--limit", type=int, default=0, help="Max transfer pairs to check (0 = all)")
    parser.add_argument("--min-distance", type=float, default=0, help="Min distance in meters between stops")
    parser.add_argument("--output", default="data/indoor_report.csv", help="Output CSV path (detailed)")
    parser.add_argument("--summary", default="data/indoor_summary.csv", help="Output CSV path (by station)")
    args = parser.parse_args()

    gtfs_dir = Path(args.gtfs_dir)
    print(f"Loading GTFS from {gtfs_dir}...")
    stops = load_stops(gtfs_dir)
    print(f"  {len(stops):,} stops loaded")

    transfers = load_transfers(gtfs_dir, stops, args.min_distance)
    print(f"  {len(transfers):,} unique transfer pairs (>={args.min_distance}m)")

    # Sort by distance descending
    transfers.sort(key=lambda x: -x[2])
    if args.limit > 0:
        transfers = transfers[:args.limit]
    print(f"  Checking {len(transfers):,} pairs against Valhalla at {args.valhalla}")

    # Prepare output CSV
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    csv_file = open(output_path, "w", newline="", encoding="utf-8")
    writer = csv.writer(csv_file)
    writer.writerow([
        "from_stop_id", "from_name", "from_station", "from_lat", "from_lon",
        "to_stop_id", "to_name", "to_station", "to_lat", "to_lon",
        "straight_distance_m", "route_distance_m", "route_duration_s",
        "has_indoor", "nb_maneuvers", "nb_indoor_maneuvers",
        "indoor_types", "indoor_instructions",
        "all_maneuver_types",
    ])

    # Stats
    total = len(transfers)
    indoor_count = 0
    outdoor_count = 0
    error_count = 0
    indoor_type_counts = defaultdict(int)
    station_stats = defaultdict(lambda: {
        "indoor_pairs": 0, "outdoor_pairs": 0, "error_pairs": 0,
        "elevators": 0, "stairs": 0, "escalators": 0,
        "enter_building": 0, "exit_building": 0,
    })

    t0 = time.time()

    for i, (from_id, to_id, dist) in enumerate(transfers):
        f_lat, f_lon, f_name, _ = stops[from_id]
        t_lat, t_lon, t_name, _ = stops[to_id]
        f_station = station_name(stops, from_id)
        t_station = station_name(stops, to_id)

        result = query_valhalla(args.valhalla, f_lat, f_lon, t_lat, t_lon)

        if result is None:
            error_count += 1
            writer.writerow([
                from_id, f_name, f_station, f_lat, f_lon,
                to_id, t_name, t_station, t_lat, t_lon,
                round(dist), "", "",
                "error", "", "",
                "", "", "",
            ])
            for st in (f_station, t_station):
                station_stats[st]["error_pairs"] += 1
            continue

        maneuvers, route_dist, route_time = result
        indoor = [(t, instr) for t, instr in maneuvers if t in INDOOR_TYPES]
        has_indoor = len(indoor) > 0

        if has_indoor:
            indoor_count += 1
        else:
            outdoor_count += 1

        indoor_types_str = "|".join(INDOOR_TYPES[t] for t, _ in indoor)
        indoor_instr_str = "|".join(instr for _, instr in indoor)
        all_types_str = "|".join(
            ALL_MANEUVER_TYPES.get(t, str(t)) for t, _ in maneuvers
        )

        writer.writerow([
            from_id, f_name, f_station, f_lat, f_lon,
            to_id, t_name, t_station, t_lat, t_lon,
            round(dist), route_dist, route_time,
            "yes" if has_indoor else "no",
            len(maneuvers), len(indoor),
            indoor_types_str, indoor_instr_str,
            all_types_str,
        ])

        # Update stats
        for t, _ in indoor:
            indoor_type_counts[INDOOR_TYPES[t]] += 1

        for st in (f_station, t_station):
            if has_indoor:
                station_stats[st]["indoor_pairs"] += 1
                for t, _ in indoor:
                    typ = INDOOR_TYPES[t]
                    if typ == "elevator":
                        station_stats[st]["elevators"] += 1
                    elif typ == "stairs":
                        station_stats[st]["stairs"] += 1
                    elif typ == "escalator":
                        station_stats[st]["escalators"] += 1
                    elif typ == "enter_building":
                        station_stats[st]["enter_building"] += 1
                    elif typ == "exit_building":
                        station_stats[st]["exit_building"] += 1
            else:
                station_stats[st]["outdoor_pairs"] += 1

        # Progress
        if (i + 1) % 100 == 0 or i + 1 == total:
            elapsed = time.time() - t0
            rate = (i + 1) / elapsed if elapsed > 0 else 0
            eta = (total - i - 1) / rate if rate > 0 else 0
            print(
                f"\r  [{i+1:,}/{total:,}] indoor: {indoor_count}, outdoor: {outdoor_count}, "
                f"errors: {error_count} | {rate:.0f} req/s | ETA: {eta:.0f}s",
                end="", flush=True,
            )

    csv_file.close()
    elapsed = time.time() - t0

    # Write station summary CSV
    summary_path = Path(args.summary)
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    with open(summary_path, "w", newline="", encoding="utf-8") as f:
        sw = csv.writer(f)
        sw.writerow([
            "station", "indoor_pairs", "outdoor_pairs", "error_pairs",
            "total_pairs", "indoor_ratio",
            "elevators", "stairs", "escalators", "enter_building", "exit_building",
            "indoor_score",
        ])
        for st in sorted(station_stats.keys(), key=lambda s: -(
            station_stats[s]["indoor_pairs"]
        )):
            s = station_stats[st]
            total_pairs = s["indoor_pairs"] + s["outdoor_pairs"] + s["error_pairs"]
            ratio = s["indoor_pairs"] / total_pairs if total_pairs > 0 else 0
            score = (
                s["elevators"] + s["stairs"] + s["escalators"]
                + s["enter_building"] + s["exit_building"]
            )
            sw.writerow([
                st, s["indoor_pairs"], s["outdoor_pairs"], s["error_pairs"],
                total_pairs, f"{ratio:.2f}",
                s["elevators"], s["stairs"], s["escalators"],
                s["enter_building"], s["exit_building"],
                score,
            ])

    # Console report
    print(f"\n\n{'='*70}")
    print(f"  INDOOR ROUTING ANALYSIS REPORT")
    print(f"{'='*70}")
    print(f"  Elapsed:          {elapsed:.1f}s ({total / elapsed:.0f} req/s)")
    print(f"  Transfer pairs:   {total:,}")
    print(f"  With indoor data: {indoor_count:,} ({indoor_count/total*100:.1f}%)")
    print(f"  Outdoor only:     {outdoor_count:,} ({outdoor_count/total*100:.1f}%)")
    print(f"  Errors:           {error_count:,}")

    if indoor_type_counts:
        print(f"\n  Indoor maneuver types:")
        for mtype, count in sorted(indoor_type_counts.items(), key=lambda x: -x[1]):
            print(f"    {mtype:20s}: {count:,}")

    # Top stations
    ranked = sorted(
        station_stats.items(),
        key=lambda x: -(x[1]["indoor_pairs"]),
    )
    indoor_stations = [(st, s) for st, s in ranked if s["indoor_pairs"] > 0]
    print(f"\n  Stations with indoor data: {len(indoor_stations)}")

    if indoor_stations:
        print(f"\n  {'Station':<40s} {'Indoor':>7s} {'Total':>7s} {'Ratio':>7s} {'Score':>7s}")
        print(f"  {'-'*40} {'-'*7} {'-'*7} {'-'*7} {'-'*7}")
        for st, s in indoor_stations[:30]:
            total_pairs = s["indoor_pairs"] + s["outdoor_pairs"] + s["error_pairs"]
            ratio = s["indoor_pairs"] / total_pairs if total_pairs > 0 else 0
            score = (s["elevators"] + s["stairs"] + s["escalators"]
                     + s["enter_building"] + s["exit_building"])
            name = st[:40]
            print(f"  {name:<40s} {s['indoor_pairs']:>7d} {total_pairs:>7d} {ratio:>6.0%} {score:>7d}")

    print(f"\n  Detailed CSV:  {output_path}")
    print(f"  Summary CSV:   {summary_path}")
    print()

    return 0


if __name__ == "__main__":
    sys.exit(main())
