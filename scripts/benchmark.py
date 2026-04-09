#!/usr/bin/env python3
"""
Glove RAPTOR Engine — Performance Benchmark

Sends concurrent journey planning requests to the Glove API with diverse
origin/destination pairs across Ile-de-France, measures response times,
and generates a performance report image.

Usage:
    python3 bin/benchmark.py [--host HOST] [--port PORT] [--rounds ROUNDS]

Requires: matplotlib (pip install matplotlib)
"""

import argparse
import json
import statistics
import time
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed

# ---------------------------------------------------------------------------
# Test scenarios — representative O/D pairs across Ile-de-France
# ---------------------------------------------------------------------------

SCENARIOS = [
    # (label, from_stop_id, to_stop_id)
    ("Châtelet → Gare de Lyon",
     "IDFM:monomodalStopPlace:45102", "IDFM:monomodalStopPlace:470195"),
    ("Gare du Nord → La Défense",
     "IDFM:monomodalStopPlace:462394", "IDFM:monomodalStopPlace:470549"),
    ("Gare Saint-Lazare → Nation",
     "IDFM:monomodalStopPlace:58566", "IDFM:monomodalStopPlace:473875"),
    ("Gare Montparnasse → Auber",
     "IDFM:monomodalStopPlace:43238", "IDFM:monomodalStopPlace:45873"),
    ("Vincennes → Versailles Chantiers",
     "IDFM:monomodalStopPlace:43224", "IDFM:monomodalStopPlace:43219"),
    ("Denfert-Rochereau → Gare du Nord",
     "IDFM:monomodalStopPlace:473890", "IDFM:monomodalStopPlace:462394"),
    ("La Défense → Créteil Pompadour",
     "IDFM:monomodalStopPlace:470549", "IDFM:monomodalStopPlace:46286"),
    ("Invalides → Nanterre Préfecture",
     "IDFM:monomodalStopPlace:470540", "IDFM:monomodalStopPlace:43169"),
    ("Massy-Verrières → Châtelet",
     "IDFM:monomodalStopPlace:47940", "IDFM:monomodalStopPlace:45102"),
    ("Gare de Lyon → Versailles Rive Droite",
     "IDFM:monomodalStopPlace:470195", "IDFM:monomodalStopPlace:44602"),
    ("Nation → Gare Montparnasse",
     "IDFM:monomodalStopPlace:473875", "IDFM:monomodalStopPlace:43238"),
    ("Auber → Vincennes",
     "IDFM:monomodalStopPlace:45873", "IDFM:monomodalStopPlace:43224"),
]

# ---------------------------------------------------------------------------
# Benchmark logic
# ---------------------------------------------------------------------------

def run_request(base_url, from_id, to_id, datetime_str):
    """Send a single journey request and return (status, latency_ms, nb_journeys)."""
    url = (f"{base_url}/api/journeys/public_transport"
           f"?from={from_id}&to={to_id}&datetime={datetime_str}")
    t0 = time.monotonic()
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read())
            latency = (time.monotonic() - t0) * 1000
            nb = len(data.get("journeys", []))
            return ("ok", latency, nb)
    except urllib.error.HTTPError as e:
        latency = (time.monotonic() - t0) * 1000
        return (f"http_{e.code}", latency, 0)
    except Exception as e:
        latency = (time.monotonic() - t0) * 1000
        return ("error", latency, 0)


def run_benchmark(base_url, rounds, concurrency, datetime_str):
    """Run the full benchmark and return results."""
    results = []  # list of (label, status, latency_ms, nb_journeys)

    print(f"\n{'='*60}")
    print(f"  Glove RAPTOR Benchmark")
    print(f"  {len(SCENARIOS)} scenarios x {rounds} rounds = {len(SCENARIOS)*rounds} requests")
    print(f"  Concurrency: {concurrency} threads")
    print(f"  Target: {base_url}")
    print(f"{'='*60}\n")

    for round_num in range(1, rounds + 1):
        print(f"  Round {round_num}/{rounds}", end="", flush=True)
        round_results = []

        with ThreadPoolExecutor(max_workers=concurrency) as pool:
            futures = {}
            for label, from_id, to_id in SCENARIOS:
                f = pool.submit(run_request, base_url, from_id, to_id, datetime_str)
                futures[f] = label

            for f in as_completed(futures):
                label = futures[f]
                status, latency, nb = f.result()
                round_results.append((label, status, latency, nb))

        results.extend(round_results)
        lats = [r[2] for r in round_results]
        print(f"  — avg {statistics.mean(lats):.0f}ms, "
              f"p95 {sorted(lats)[int(len(lats)*0.95)]:.0f}ms")

    return results


def print_summary(results):
    """Print a text summary table."""
    ok_results = [r for r in results if r[1] == "ok"]
    err_results = [r for r in results if r[1] != "ok"]
    latencies = [r[2] for r in ok_results]

    print(f"\n{'='*60}")
    print(f"  Results: {len(ok_results)} OK, {len(err_results)} errors")
    if latencies:
        print(f"  Min:    {min(latencies):>8.1f} ms")
        print(f"  Avg:    {statistics.mean(latencies):>8.1f} ms")
        print(f"  Median: {statistics.median(latencies):>8.1f} ms")
        print(f"  p95:    {sorted(latencies)[int(len(latencies)*0.95)]:>8.1f} ms")
        print(f"  p99:    {sorted(latencies)[int(len(latencies)*0.99)]:>8.1f} ms")
        print(f"  Max:    {max(latencies):>8.1f} ms")
    print(f"{'='*60}")

    # Per-scenario breakdown
    print(f"\n  {'Scenario':<40} {'Avg':>7} {'p95':>7} {'Max':>7} {'#J':>3}")
    print(f"  {'-'*40} {'-'*7} {'-'*7} {'-'*7} {'-'*3}")
    by_scenario = {}
    for label, status, latency, nb in ok_results:
        by_scenario.setdefault(label, []).append((latency, nb))
    for label, vals in sorted(by_scenario.items()):
        lats = [v[0] for v in vals]
        avg_j = statistics.mean([v[1] for v in vals])
        p95 = sorted(lats)[int(len(lats) * 0.95)]
        print(f"  {label:<40} {statistics.mean(lats):>6.0f}ms {p95:>6.0f}ms "
              f"{max(lats):>6.0f}ms {avg_j:>3.0f}")

    return latencies, by_scenario


def generate_chart(latencies, by_scenario, output_path):
    """Generate a performance chart image."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.ticker as mticker

    fig, axes = plt.subplots(1, 2, figsize=(14, 5.5),
                             gridspec_kw={"width_ratios": [1, 1.4]})
    fig.patch.set_facecolor("#0a0a12")

    for ax in axes:
        ax.set_facecolor("#12121e")
        ax.tick_params(colors="#8b89a0", labelsize=9)
        ax.spines["top"].set_visible(False)
        ax.spines["right"].set_visible(False)
        for spine in ax.spines.values():
            spine.set_color("#2a2a3a")

    # --- Left: histogram ---
    ax = axes[0]
    ax.hist(latencies, bins=30, color="#00e5ff", alpha=0.85, edgecolor="#0a0a12", linewidth=0.5)
    ax.set_xlabel("Response time (ms)", color="#8b89a0", fontsize=10)
    ax.set_ylabel("Requests", color="#8b89a0", fontsize=10)
    ax.set_title("Response Time Distribution", color="#e8e6f0", fontsize=12,
                 fontweight="bold", pad=12)
    ax.axvline(statistics.median(latencies), color="#ffb800", linewidth=1.5,
               linestyle="--", label=f"Median: {statistics.median(latencies):.0f}ms")
    p95 = sorted(latencies)[int(len(latencies) * 0.95)]
    ax.axvline(p95, color="#ff5252", linewidth=1.5,
               linestyle="--", label=f"p95: {p95:.0f}ms")
    legend = ax.legend(fontsize=9, loc="upper right",
                       facecolor="#1a1a2e", edgecolor="#2a2a3a", labelcolor="#e8e6f0")

    # --- Right: horizontal bar chart by scenario ---
    ax = axes[1]
    labels = []
    avgs = []
    p95s = []
    for label in sorted(by_scenario.keys()):
        lats = [v[0] for v in by_scenario[label]]
        labels.append(label)
        avgs.append(statistics.mean(lats))
        p95s.append(sorted(lats)[int(len(lats) * 0.95)])

    y_pos = range(len(labels))
    ax.barh(y_pos, p95s, height=0.6, color="#ff5252", alpha=0.35, label="p95")
    ax.barh(y_pos, avgs, height=0.6, color="#00e5ff", alpha=0.85, label="Avg")
    ax.set_yticks(y_pos)
    ax.set_yticklabels(labels, fontsize=8, color="#e8e6f0")
    ax.set_xlabel("Response time (ms)", color="#8b89a0", fontsize=10)
    ax.set_title("Average Response Time by Route", color="#e8e6f0", fontsize=12,
                 fontweight="bold", pad=12)
    ax.invert_yaxis()
    legend2 = ax.legend(fontsize=9, loc="lower right",
                        facecolor="#1a1a2e", edgecolor="#2a2a3a", labelcolor="#e8e6f0")

    # Stats box
    stats_text = (f"Total: {len(latencies)} requests\n"
                  f"Min: {min(latencies):.0f}ms\n"
                  f"Avg: {statistics.mean(latencies):.0f}ms\n"
                  f"p95: {p95:.0f}ms\n"
                  f"Max: {max(latencies):.0f}ms")
    fig.text(0.01, 0.01, stats_text, fontsize=8, color="#56546a",
             fontfamily="monospace", verticalalignment="bottom")

    fig.suptitle("Glove RAPTOR Engine — Performance Benchmark",
                 color="#e8e6f0", fontsize=14, fontweight="bold", y=0.98)
    plt.tight_layout(rect=[0, 0.05, 1, 0.94])
    plt.savefig(output_path, dpi=150, facecolor=fig.get_facecolor(),
                bbox_inches="tight", pad_inches=0.3)
    plt.close()
    print(f"\n  Chart saved to {output_path}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Glove RAPTOR benchmark")
    parser.add_argument("--host", default="localhost", help="API host (default: localhost)")
    parser.add_argument("--port", type=int, default=8080, help="API port (default: 8080)")
    parser.add_argument("--rounds", type=int, default=5, help="Number of rounds (default: 5)")
    parser.add_argument("--concurrency", type=int, default=4, help="Concurrent threads (default: 4)")
    parser.add_argument("--datetime", default="20260406T083000", help="Departure datetime")
    parser.add_argument("--output", default="docs/benchmark.png", help="Output image path")
    args = parser.parse_args()

    base_url = f"http://{args.host}:{args.port}"

    # Quick health check
    try:
        urllib.request.urlopen(f"{base_url}/api/status", timeout=5)
    except Exception:
        print(f"  ERROR: Cannot reach {base_url}/api/status")
        print(f"  Make sure Glove is running: cargo run --release")
        exit(1)

    results = run_benchmark(base_url, args.rounds, args.concurrency, args.datetime)
    latencies, by_scenario = print_summary(results)

    if latencies:
        generate_chart(latencies, by_scenario, args.output)


if __name__ == "__main__":
    main()
