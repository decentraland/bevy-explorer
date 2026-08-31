#!/usr/bin/env python3
"""Run N scenes in ONE orchestrated headless engine and sample it.

Usage: multi.py --bin <path> --scenes <jsonl> --duration <secs> --out <dir>

<jsonl>: one {"world","sceneId","urn"} per line (add-scene is sent for each).
Spawns `<bin> --orchestrated --realm <realm>`, feeds add-scene over stdin like
multiplayer-server's bevy-engine-process, samples engine+sidecar every 5s, and
derives per-scene tick rates from `@bevy-ctl {"type":"scene-status",...}` events.
"""
import argparse
import csv
import json
import os
import signal
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from sample import ps_metrics, thread_count, find_sidecar  # noqa: E402

SAMPLE_PERIOD = 5.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--scenes", required=True)
    ap.add_argument("--realm", default="https://realm-provider.decentraland.org/main")
    ap.add_argument("--duration", type=float, default=300.0)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    scenes = [json.loads(l) for l in open(args.scenes) if l.strip()]
    os.makedirs(args.out, exist_ok=True)
    log_path = os.path.join(args.out, "multi.log")
    csv_path = os.path.join(args.out, "multi.csv")
    json_path = os.path.join(args.out, "multi.json")

    log_f = open(log_path, "w")
    proc = subprocess.Popen(
        [args.bin, "--orchestrated", "--realm", args.realm],
        stdin=subprocess.PIPE, stdout=log_f, stderr=subprocess.STDOUT, text=True,
    )
    start = time.time()
    for s in scenes:
        proc.stdin.write(json.dumps({
            "type": "add-scene", "sceneId": s["sceneId"], "urn": s["urn"],
        }) + "\n")
    proc.stdin.flush()

    sidecar = None
    with open(csv_path, "w", newline="") as cf:
        w = csv.writer(cf)
        w.writerow(["t", "engine_rss_kib", "engine_cpu_s", "engine_pcpu", "engine_threads",
                    "sidecar_rss_kib", "sidecar_cpu_s", "sidecar_pcpu", "sidecar_threads"])
        while proc.poll() is None and time.time() - start < args.duration:
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

    # closing stdin = orchestrator disconnect; engine exits like hammurabi workers
    try:
        proc.stdin.close()
        proc.wait(timeout=15)
    except subprocess.TimeoutExpired:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
    log_f.close()

    # per-scene status timeline from @bevy-ctl events (emitted every ~5s)
    status = {}  # scene -> [(seq, tick, broken)]
    live, failed = set(), {}
    for line in open(log_path, errors="replace"):
        idx = line.find("@bevy-ctl ")
        if idx < 0:
            continue
        try:
            ev = json.loads(line[idx + len("@bevy-ctl "):])
        except json.JSONDecodeError:
            continue
        ty = ev.get("type")
        if ty == "scene-status":
            status.setdefault(ev["scene"], []).append((ev["tick"], ev.get("broken", False)))
        elif ty == "scene-live":
            live.add(ev["scene"])
        elif ty in ("scene-failed", "scene-broken", "error"):
            failed.setdefault(ev.get("scene", "?"), []).append(ev.get("error", ty))

    by_hash = {s["sceneId"]: s["world"] for s in scenes}
    per_scene = {}
    for scene, points in status.items():
        # steady window: skip the first 12 status lines (~60s), Δtick over ~5s cadence
        pts = [p for p in points if not p[1]]
        rate = None
        if len(pts) > 14:
            head, tail = pts[12], pts[-1]
            rate = round((tail[0] - head[0]) / (5.0 * (len(pts) - 1 - 12)), 2)
        per_scene[by_hash.get(scene, scene)] = {
            "ticks": points[-1][0] if points else 0,
            "tick_hz": rate,
            "broken": any(p[1] for p in points),
            "live": scene in live,
        }

    summary = {
        "scenes": len(scenes),
        "live": len(live),
        "wall_secs": round(time.time() - start, 1),
        "exit_code": proc.returncode,
        "per_scene": per_scene,
        "events": {k: v[:3] for k, v in failed.items()},
    }
    with open(json_path, "w") as f:
        json.dump(summary, f, indent=2)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
