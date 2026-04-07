#!/usr/bin/env python3
"""Check which GTFS transfer pairs have indoor routing data in Valhalla.

Queries Valhalla's pedestrian route API for transfer stop pairs and checks
whether the response contains indoor maneuver types (elevator, stairs,
escalator, enter/exit building).

Usage:
    python3 bin/check_indoor.py [--valhalla http://localhost:8002] [--limit 500] [--min-distance 50]
"""

import argparse
import csv
import json
import math
import sys
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

def haversine(lat1, lon1, lat2, lon2):
    """Distance in meters between two coordinates."""
    R = 6_371_000
    dlat = math.radians(lat2 - lat1)
    dlon = math.radians(lon2 - lon1)
    a = math.sin(dlat / 2) ** 2 + math.cos(math.radians(lat1)) * math.cos(math.radians(lat2)) * math.sin(dlon / 2) ** 2
    return R * 2 * math.atan2(math.sqrt(a), math.sqrt(1 - a))

def load_stops(gtfs_dir):
    """Load stops from GTFS stops.txt into {stop_id: (lat, lon, name)}."""
    stops = {}
    with open(gtfs_dir / "stops.txt", encoding="utf-8-sig") as f:
        for row in csv.DictReader(f):
            try:
                stops[row["stop_id"]] = (
                    float(row["stop_lat"]),
                    float(row["stop_lon"]),
                    row.get("stop_name", ""),
                )
            except (ValueError, KeyError):
                continue
    return stops

def load_transfers(gtfs_dir, stops, min_distance):
    """Load unique transfer pairs with distance > min_distance meters."""
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
            f_lat, f_lon, _ = stops[from_id]
            t_lat, t_lon, _ = stops[to_id]
            dist = haversine(f_lat, f_lon, t_lat, t_lon)
            if dist >= min_distance:
                pairs.append((from_id, to_id, dist))
    return pairs

def query_valhalla(base_url, from_lat, from_lon, to_lat, to_lon):
    """Query Valhalla pedestrian route, return maneuver types or None."""
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
        with urllib.request.urlopen(req, timeout=5) as resp:
            data = json.loads(resp.read())
            maneuvers = data.get("trip", {}).get("legs", [{}])[0].get("maneuvers", [])
            return [(m["type"], m.get("instruction", "")) for m in maneuvers]
    except Exception:
        return None

def main():
    parser = argparse.ArgumentParser(description="Check indoor routing data in Valhalla")
    parser.add_argument("--valhalla", default="http://localhost:8002", help="Valhalla base URL")
    parser.add_argument("--gtfs-dir", default="data/gtfs", help="GTFS data directory")
    parser.add_argument("--limit", type=int, default=500, help="Max transfer pairs to check")
    parser.add_argument("--min-distance", type=float, default=50, help="Min distance in meters between stops")
    args = parser.parse_args()

    gtfs_dir = Path(args.gtfs_dir)
    print(f"Loading GTFS from {gtfs_dir}...")
    stops = load_stops(gtfs_dir)
    print(f"  {len(stops)} stops loaded")

    transfers = load_transfers(gtfs_dir, stops, args.min_distance)
    print(f"  {len(transfers)} transfer pairs (>{args.min_distance}m)")

    # Sort by distance descending (longer transfers more likely to have indoor data)
    transfers.sort(key=lambda x: -x[2])
    transfers = transfers[:args.limit]
    print(f"  Checking {len(transfers)} pairs against Valhalla at {args.valhalla}\n")

    indoor_found = []
    outdoor_only = 0
    errors = 0
    indoor_type_counts = defaultdict(int)

    for i, (from_id, to_id, dist) in enumerate(transfers):
        f_lat, f_lon, f_name = stops[from_id]
        t_lat, t_lon, t_name = stops[to_id]

        maneuvers = query_valhalla(args.valhalla, f_lat, f_lon, t_lat, t_lon)

        if maneuvers is None:
            errors += 1
            continue

        indoor_maneuvers = [(t, instr) for t, instr in maneuvers if t in INDOOR_TYPES]

        if indoor_maneuvers:
            indoor_found.append({
                "from": f_name,
                "to": t_name,
                "distance": round(dist),
                "indoor_maneuvers": [(INDOOR_TYPES[t], instr) for t, instr in indoor_maneuvers],
            })
            for t, _ in indoor_maneuvers:
                indoor_type_counts[INDOOR_TYPES[t]] += 1
        else:
            outdoor_only += 1

        # Progress
        if (i + 1) % 50 == 0 or i + 1 == len(transfers):
            print(f"  [{i+1}/{len(transfers)}] indoor: {len(indoor_found)}, outdoor: {outdoor_only}, errors: {errors}", end="\r")

    print(f"\n\n{'='*60}")
    print(f"RESULTS")
    print(f"{'='*60}")
    print(f"  Total checked:    {len(transfers)}")
    print(f"  Indoor routing:   {len(indoor_found)}")
    print(f"  Outdoor only:     {outdoor_only}")
    print(f"  Errors:           {errors}")

    if indoor_type_counts:
        print(f"\n  Indoor maneuver types found:")
        for mtype, count in sorted(indoor_type_counts.items(), key=lambda x: -x[1]):
            print(f"    {mtype:20s}: {count}")

    if indoor_found:
        print(f"\n{'='*60}")
        print(f"STATIONS WITH INDOOR ROUTING ({len(indoor_found)})")
        print(f"{'='*60}")
        for entry in indoor_found:
            types = ", ".join(set(t for t, _ in entry["indoor_maneuvers"]))
            print(f"\n  {entry['from']} -> {entry['to']} ({entry['distance']}m)")
            print(f"    Types: {types}")
            for mtype, instr in entry["indoor_maneuvers"]:
                print(f"    - [{mtype}] {instr}")
    else:
        print(f"\n  No indoor routing data found in Valhalla tiles.")
        print(f"  This means OSM does not contain connected indoor paths")
        print(f"  (corridors, stairs, elevators) for these stations.")

    return 0 if not indoor_found else 0

if __name__ == "__main__":
    sys.exit(main())
