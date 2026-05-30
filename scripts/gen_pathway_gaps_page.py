#!/usr/bin/env python3
"""
Generate the mdBook page ranking GTFS pathway vs Valhalla indoor time gaps.

Reads the CSV produced by `scripts/pathway_valhalla_diff.py` and `stops.txt`
(for names + station hierarchy), then writes:
  - a chart (histogram of Δ + GTFS-vs-Valhalla scatter) to book/src/images/,
  - a Markdown page with a summary, a per-station ranking, and the largest
    individual gaps (sorted by absolute gap, descending).

Usage:
    python3 scripts/gen_pathway_gaps_page.py \
        [--csv scripts/pathway_diff.csv] [--gtfs data/gtfs] \
        [--top 50] [--top-stations 30] \
        [--output book/src/idfm/pathway-time-gaps.md] \
        [--chart book/src/images/pathway_gaps.png]
"""

import argparse
import csv
import statistics


def load_stops(gtfs_dir):
    """stop_id -> {name, parent}."""
    stops = {}
    with open(f"{gtfs_dir}/stops.txt", newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            stops[r["stop_id"]] = {
                "name": (r.get("stop_name") or "").strip(),
                "parent": (r.get("parent_station") or "").strip(),
            }
    return stops


def name_of(stops, stop_id):
    s = stops.get(stop_id)
    return s["name"] if s and s["name"] else stop_id


def resolve_station(stops, stop_id):
    """Walk the parent_station chain to the top; return (station_id, name)."""
    seen = set()
    cur = stop_id
    while cur and cur not in seen:
        seen.add(cur)
        s = stops.get(cur)
        if not s or not s["parent"]:
            break
        cur = s["parent"]
    return cur, name_of(stops, cur)


def pathway_station(stops, row):
    """Best-effort station for a pathway: the deepest endpoint with a parent."""
    for end in (row["to"], row["from"]):
        sid, sname = resolve_station(stops, end)
        if sid and stops.get(sid, {}).get("name"):
            return sid, sname
    sid, sname = resolve_station(stops, row["from"])
    return sid, sname


def generate_chart(indoor, path):
    """Histogram of Δ + GTFS-vs-Valhalla scatter, dark theme."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    diffs = [r["diff_s"] for r in indoor]
    clipped = [max(-200, min(200, d)) for d in diffs]

    fig, axes = plt.subplots(1, 2, figsize=(14, 5))
    fig.patch.set_facecolor("#0a0a12")
    for ax in axes:
        ax.set_facecolor("#12121e")
        ax.tick_params(colors="#8b89a0", labelsize=9)
        ax.spines["top"].set_visible(False)
        ax.spines["right"].set_visible(False)
        for sp in ax.spines.values():
            sp.set_color("#2a2a3a")

    ax = axes[0]
    ax.hist(clipped, bins=40, color="#00e5ff", alpha=0.85, edgecolor="#0a0a12", linewidth=0.5)
    ax.axvline(0, color="#56546a", linewidth=1)
    med = statistics.median(diffs)
    ax.axvline(max(-200, min(200, med)), color="#ffb800", linewidth=1.5, linestyle="--",
               label=f"Median: {med:+.0f}s")
    ax.set_xlabel("Δ = Valhalla − GTFS (s, clipped to ±200)", color="#8b89a0", fontsize=10)
    ax.set_ylabel("Pathways", color="#8b89a0", fontsize=10)
    ax.set_title("Distribution of time gaps", color="#e8e6f0", fontsize=12, fontweight="bold", pad=12)
    ax.legend(fontsize=9, facecolor="#1a1a2e", edgecolor="#2a2a3a", labelcolor="#e8e6f0")

    ax = axes[1]
    gx = [r["gtfs_traversal_s"] for r in indoor]
    vy = [r["valhalla_s"] for r in indoor]
    ax.scatter(gx, vy, s=12, color="#00e5ff", alpha=0.4, edgecolors="none")
    hi = max(max(gx), max(vy))
    ax.plot([0, hi], [0, hi], color="#ff5252", linewidth=1.2, linestyle="--", label="Valhalla = GTFS")
    ax.set_xlabel("GTFS traversal_time (s)", color="#8b89a0", fontsize=10)
    ax.set_ylabel("Valhalla indoor time (s)", color="#8b89a0", fontsize=10)
    ax.set_title("GTFS vs Valhalla (indoor pathways)", color="#e8e6f0", fontsize=12, fontweight="bold", pad=12)
    ax.legend(fontsize=9, facecolor="#1a1a2e", edgecolor="#2a2a3a", labelcolor="#e8e6f0")

    fig.suptitle("GTFS pathways vs Valhalla indoor walking time",
                 color="#e8e6f0", fontsize=14, fontweight="bold", y=0.99)
    plt.tight_layout(rect=[0, 0, 1, 0.95])
    plt.savefig(path, dpi=150, facecolor=fig.get_facecolor(), bbox_inches="tight", pad_inches=0.3)
    plt.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--csv", default="scripts/pathway_diff.csv")
    ap.add_argument("--gtfs", default="data/gtfs")
    ap.add_argument("--top", type=int, default=50)
    ap.add_argument("--top-stations", type=int, default=30)
    ap.add_argument("--min-pathways", type=int, default=3,
                    help="min indoor pathways for a station to be ranked")
    ap.add_argument("--output", default="book/src/idfm/pathway-time-gaps.md")
    ap.add_argument("--chart", default="book/src/images/pathway_gaps.png")
    args = ap.parse_args()

    stops = load_stops(args.gtfs)
    rows = list(csv.DictReader(open(args.csv, newline="", encoding="utf-8")))
    for r in rows:
        r["diff_s"] = int(r["diff_s"])
        r["gtfs_traversal_s"] = int(r["gtfs_traversal_s"])
        r["valhalla_s"] = int(r["valhalla_s"])
        r["length_m"] = float(r["length_m"]) if r["length_m"] else None
        r["valhalla_m"] = float(r["valhalla_m"]) if r.get("valhalla_m") else None

    indoor = [r for r in rows if r["indoor"] == "True"]
    indoor.sort(key=lambda r: abs(r["diff_s"]), reverse=True)
    total = len(indoor)
    diffs = [r["diff_s"] for r in indoor]
    ab = [abs(d) for d in diffs]

    generate_chart(indoor, args.chart)

    # Per-station aggregation
    by_station = {}
    for r in indoor:
        _, sname = pathway_station(stops, r)
        by_station.setdefault(sname, []).append(r["diff_s"])
    stations = []
    for name, ds in by_station.items():
        if len(ds) < args.min_pathways:
            continue
        a = [abs(d) for d in ds]
        stations.append({
            "name": name, "n": len(ds),
            "median": statistics.median(ds),
            "mean_abs": statistics.mean(a),
            "worst": max(ds, key=abs),
        })
    stations.sort(key=lambda s: s["mean_abs"], reverse=True)

    def pct(th):
        n = sum(1 for d in ab if d > th)
        return f"{n} ({100 * n / total:.0f}%)" if total else "0"

    L = []
    L.append("# Pathway Time Gaps\n")
    L.append("This page ranks the **largest discrepancies** between the GTFS "
             "`pathways.txt` `traversal_time` and the indoor walking time computed "
             "by Valhalla for the same two station nodes.\n")
    L.append("""```admonish info title="Methodology"
For every pathway with a `traversal_time`, Valhalla is asked for a pedestrian
route between the two nodes using Glove's indoor-friendly costing
(`step_penalty: 0`, `elevator_penalty: 0`, `use_tunnels: 1.0`). A pathway is kept
here only when Valhalla's route **actually uses indoor infrastructure** — i.e. it
contains an elevator / stairs / escalator / building enter-exit maneuver. Δ is
`Valhalla − GTFS`: a positive value means Valhalla is slower than the GTFS
declared time.

Generated by `scripts/pathway_valhalla_diff.py` + `scripts/gen_pathway_gaps_page.py`.
```
""")

    L.append("![Pathway time gaps](../images/pathway_gaps.png)\n")

    L.append("## Summary\n")
    L.append("| Metric | Value |")
    L.append("|--------|-------|")
    L.append(f"| Pathways with indoor data | {total} |")
    if total:
        L.append(f"| Median Δ (Valhalla − GTFS) | {statistics.median(diffs):+.0f} s |")
        L.append(f"| Mean \\|Δ\\| | {statistics.mean(ab):.0f} s |")
        L.append(f"| \\|Δ\\| ≤ 15 s (good agreement) | {sum(1 for d in ab if d <= 15)} ({100*sum(1 for d in ab if d<=15)/total:.0f}%) |")
        L.append(f"| \\|Δ\\| > 30 s | {pct(30)} |")
        L.append(f"| \\|Δ\\| > 60 s | {pct(60)} |")
        L.append(f"| \\|Δ\\| > 120 s | {pct(120)} |")
    L.append("")

    # Per-station ranking
    L.append(f"## Worst stations (≥ {args.min_pathways} indoor pathways)\n")
    L.append("Stations ranked by mean absolute gap — the highest rows are the "
             "stations whose indoor modelling diverges most from GTFS.\n")
    L.append("| # | Station | Pathways | Median Δ | Mean \\|Δ\\| | Worst Δ |")
    L.append("|---|---------|---------:|---------:|-----------:|--------:|")
    for i, s in enumerate(stations[: args.top_stations], 1):
        L.append(f"| {i} | {s['name']} | {s['n']} | {s['median']:+.0f} s | "
                 f"{s['mean_abs']:.0f} s | {s['worst']:+d} s |")
    L.append("")

    # Individual ranking
    L.append(f"## Top {min(args.top, total)} individual gaps (descending)\n")
    L.append("Sorted by absolute gap. `GTFS dist` is the declared pathway length; "
             "`Valh dist` is the distance Valhalla actually walked — a much larger "
             "`Valh dist` reveals an indoor detour (incomplete OSM connectivity), "
             "while a large negative Δ suggests an over-cautious GTFS `traversal_time`.\n")
    L.append("| # | From → To | GTFS dist | Valh dist | GTFS | Valhalla | Δ |")
    L.append("|---|-----------|----------:|----------:|-----:|---------:|--:|")
    for i, r in enumerate(indoor[: args.top], 1):
        gd = f"{r['length_m']:.0f} m" if r["length_m"] else "—"
        vd = f"{r['valhalla_m']:.0f} m" if r["valhalla_m"] else "—"
        L.append(f"| {i} | {name_of(stops, r['from'])} → {name_of(stops, r['to'])} | "
                 f"{gd} | {vd} | {r['gtfs_traversal_s']} s | {r['valhalla_s']} s | "
                 f"**{r['diff_s']:+d} s** |")
    L.append("")
    L.append("```admonish note\n"
             f"The full ranking of all {total} indoor pathways (plus pathways with "
             "no indoor OSM data) is in `scripts/pathway_diff.csv`.\n```\n")

    with open(args.output, "w", encoding="utf-8") as f:
        f.write("\n".join(L))
    print(f"Wrote {args.output} + {args.chart} "
          f"({total} indoor pathways, {len(stations)} stations ranked).")


if __name__ == "__main__":
    main()
