#!/usr/bin/env python3
"""Run the headless engine for one world and sample engine+sidecar metrics.

Usage: sample.py --bin <path> --realm <world> --duration <secs> --out <dir> [--extra-args "..."]

Samples every 5s via `ps`: RSS (KiB), cumulative CPU time, %CPU and thread count
for the engine process and its dcl_deno_ipc sidecar child. Writes:
  <out>/<world>.csv     one row per sample
  <out>/<world>.log     engine stdout/stderr
  <out>/<world>.json    run summary (exit code, wall time, scene ticks parsed from log)
"""
import argparse
import csv
import json
import os
import re
import signal
import subprocess
import sys
import time

SAMPLE_PERIOD = 5.0


def ps_metrics(pid: int):
    """Return (rss_kib, cputime_secs, pcpu) or None if the process is gone."""
    try:
        out = subprocess.check_output(
            ["ps", "-o", "rss=,time=,%cpu=", "-p", str(pid)], text=True
        ).strip()
    except subprocess.CalledProcessError:
        return None
    if not out:
        return None
    parts = out.split()
    rss = int(parts[0])
    t = parts[1]
    # time format [dd-]hh:mm:ss.cc or mm:ss.cc
    secs = 0.0
    if "-" in t:
        days, t = t.split("-", 1)
        secs += int(days) * 86400
    fields = [float(x) for x in t.split(":")]
    for f in fields:
        secs = secs * 60 + f
    pcpu = float(parts[2])
    return rss, secs, pcpu


def thread_count(pid: int):
    try:
        out = subprocess.check_output(["ps", "-M", "-p", str(pid)], text=True)
        return max(0, len(out.strip().splitlines()) - 1)
    except subprocess.CalledProcessError:
        return 0


def find_sidecar(engine_pid: int):
    try:
        out = subprocess.check_output(["pgrep", "-P", str(engine_pid)], text=True)
    except subprocess.CalledProcessError:
        return None
    for child in out.split():
        try:
            cmd = subprocess.check_output(
                ["ps", "-o", "comm=", "-p", child], text=True
            ).strip()
        except subprocess.CalledProcessError:
            continue
        if "dcl_deno_ipc" in cmd:
            return int(child)
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--realm", required=True)
    ap.add_argument("--duration", type=float, default=300.0)
    ap.add_argument("--out", required=True)
    ap.add_argument("--extra-args", default="")
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)
    world = args.realm.replace("/", "_")
    log_path = os.path.join(args.out, f"{world}.log")
    csv_path = os.path.join(args.out, f"{world}.csv")
    json_path = os.path.join(args.out, f"{world}.json")

    cmd = [args.bin, "--realm", args.realm, "--server-mode",
           "--timeout", str(int(args.duration))]
    if args.extra_args:
        cmd += args.extra_args.split()

    log_f = open(log_path, "w")
    start = time.time()
    proc = subprocess.Popen(cmd, stdout=log_f, stderr=subprocess.STDOUT)
    sidecar = None

    with open(csv_path, "w", newline="") as cf:
        w = csv.writer(cf)
        w.writerow(["t", "engine_rss_kib", "engine_cpu_s", "engine_pcpu", "engine_threads",
                    "sidecar_rss_kib", "sidecar_cpu_s", "sidecar_pcpu", "sidecar_threads"])
        while proc.poll() is None:
            time.sleep(SAMPLE_PERIOD)
            if sidecar is None:
                sidecar = find_sidecar(proc.pid)
            em = ps_metrics(proc.pid)
            sm = ps_metrics(sidecar) if sidecar else None
            row = [round(time.time() - start, 1)]
            row += list(em) + [thread_count(proc.pid)] if em else [0, 0, 0, 0]
            row += list(sm) + [thread_count(sidecar)] if sm else [0, 0, 0, 0]
            w.writerow(row)
            cf.flush()
            if time.time() - start > args.duration + 60:
                # engine failed to honor --timeout; kill it
                proc.send_signal(signal.SIGTERM)
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                break

    wall = time.time() - start
    log_f.close()

    # parse per-scene liveness from the supervisor lines
    ticks = []
    live_scenes = 0
    broken = 0
    with open(log_path) as f:
        for line in f:
            m = re.search(r"alive: (\d+) scene\(s\), max_tick=(\d+), t=(\d+)s", line)
            if m:
                live_scenes = max(live_scenes, int(m.group(1)))
                ticks.append((int(m.group(3)), int(m.group(2))))
            if "is broken" in line:
                broken += 1

    tick_rate = None
    if len(ticks) >= 2:
        # steady-state: last sample minus sample closest to t=60
        t0 = min(ticks, key=lambda x: abs(x[0] - 60))
        t1 = ticks[-1]
        if t1[0] > t0[0]:
            tick_rate = (t1[1] - t0[1]) / (t1[0] - t0[0])

    summary = {
        "realm": args.realm,
        "exit_code": proc.returncode,
        "wall_secs": round(wall, 1),
        "live_scenes": live_scenes,
        "broken_events": broken,
        "steady_tick_hz": round(tick_rate, 2) if tick_rate is not None else None,
    }
    with open(json_path, "w") as f:
        json.dump(summary, f, indent=2)
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    sys.exit(main())
