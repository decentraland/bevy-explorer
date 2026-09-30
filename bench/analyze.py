#!/usr/bin/env python3
"""Summarize bench runs: one row per world from <out>/<world>.{csv,json,log}.

Steady state = samples after WARMUP_SECS. Reports RSS (median/peak, engine+sidecar),
CPU (mean %cpu per process and combined), threads, achieved tick Hz, storage/fetch
error counts and scene log error counts.
"""
import csv
import json
import os
import re
import statistics
import sys

WARMUP_SECS = 60.0


def pct(values, p):
    if not values:
        return 0
    values = sorted(values)
    k = min(len(values) - 1, int(round(p / 100 * (len(values) - 1))))
    return values[k]


def analyze(out_dir, world):
    csv_path = os.path.join(out_dir, f"{world}.csv")
    json_path = os.path.join(out_dir, f"{world}.json")
    log_path = os.path.join(out_dir, f"{world}.log")
    if not os.path.exists(csv_path):
        return None

    rows = []
    with open(csv_path) as f:
        for row in csv.DictReader(f):
            try:
                rows.append({k: float(v) for k, v in row.items()})
            except (ValueError, TypeError):
                continue

    steady = [r for r in rows if r["t"] >= WARMUP_SECS] or rows
    tot_rss = [(r["engine_rss_kib"] + r["sidecar_rss_kib"]) / 1024 for r in steady]
    eng_rss = [r["engine_rss_kib"] / 1024 for r in steady]
    sc_rss = [r["sidecar_rss_kib"] / 1024 for r in steady]
    eng_cpu = [r["engine_pcpu"] for r in steady]
    sc_cpu = [r["sidecar_pcpu"] for r in steady]
    threads = [r["engine_threads"] + r["sidecar_threads"] for r in steady]

    summary = {}
    if os.path.exists(json_path):
        summary = json.load(open(json_path))

    storage_errors = fetch_errors = scene_errors = 0
    if os.path.exists(log_path):
        for line in open(log_path, errors="replace"):
            if "storage.decentraland" in line and ("ERROR" in line or "error" in line):
                storage_errors += 1
            elif "ERROR" in line and "renderer_context" in line:
                scene_errors += 1
            if "fetch" in line.lower() and "error" in line.lower():
                fetch_errors += 1

    # memory growth: linear slope over steady window (MB/min)
    slope = None
    if len(steady) >= 4:
        xs = [r["t"] / 60 for r in steady]
        ys = tot_rss
        n = len(xs)
        mx, my = sum(xs) / n, sum(ys) / n
        denom = sum((x - mx) ** 2 for x in xs)
        if denom > 0:
            slope = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / denom

    return {
        "world": world,
        "exit_code": summary.get("exit_code"),
        "tick_hz": summary.get("steady_tick_hz"),
        "broken": summary.get("broken_events", 0),
        "rss_total_med_mb": round(statistics.median(tot_rss), 1) if tot_rss else None,
        "rss_total_p95_mb": round(pct(tot_rss, 95), 1) if tot_rss else None,
        "rss_engine_med_mb": round(statistics.median(eng_rss), 1) if eng_rss else None,
        "rss_sidecar_med_mb": round(statistics.median(sc_rss), 1) if sc_rss else None,
        "rss_slope_mb_per_min": round(slope, 2) if slope is not None else None,
        "cpu_engine_mean_pct": round(statistics.mean(eng_cpu), 1) if eng_cpu else None,
        "cpu_sidecar_mean_pct": round(statistics.mean(sc_cpu), 1) if sc_cpu else None,
        "threads_med": int(statistics.median(threads)) if threads else None,
        "storage_error_lines": storage_errors,
        "scene_error_lines": scene_errors,
        "samples": len(steady),
    }


def main():
    out_dir = sys.argv[1]
    worlds = sys.argv[2:] or sorted(
        f[:-4] for f in os.listdir(out_dir) if f.endswith(".csv")
    )
    results = [r for w in worlds if (r := analyze(out_dir, w))]
    print(json.dumps(results, indent=2))

    cols = ["world", "tick_hz", "rss_total_med_mb", "rss_engine_med_mb",
            "rss_sidecar_med_mb", "rss_slope_mb_per_min", "cpu_engine_mean_pct",
            "cpu_sidecar_mean_pct", "threads_med", "storage_error_lines", "broken"]
    print("\n| " + " | ".join(cols) + " |", file=sys.stderr)
    print("|" + "---|" * len(cols), file=sys.stderr)
    for r in results:
        print("| " + " | ".join(str(r.get(c, "")) for c in cols) + " |", file=sys.stderr)


if __name__ == "__main__":
    main()
