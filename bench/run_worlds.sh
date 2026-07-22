#!/bin/bash
# Sequential per-world headless benchmark. One world at a time so samples are
# not skewed by co-running engines.
# Usage: run_worlds.sh <headless-bin> <out-dir> [duration-secs] [world...]
set -u

BIN="${1:?headless binary path}"
OUT="${2:?output dir}"
DURATION="${3:-300}"
shift 3 2>/dev/null || shift $#

WORLDS=("$@")
if [ ${#WORLDS[@]} -eq 0 ]; then
  WORLDS=(cleantheclub.dcl.eth skychaser.dcl.eth towerofmadness.dcl.eth boedo.dcl.eth flagtag.dcl.eth fastlane.dcl.eth kickoff.dcl.eth)
fi

mkdir -p "$OUT"
DIR="$(cd "$(dirname "$0")" && pwd)"

for w in "${WORLDS[@]}"; do
  echo "=== $w ($(date +%H:%M:%S)) ==="
  python3 "$DIR/sample.py" --bin "$BIN" --realm "$w" --duration "$DURATION" --out "$OUT"
  echo ""
done

echo "All runs complete. Summaries:"
for w in "${WORLDS[@]}"; do
  [ -f "$OUT/$w.json" ] && cat "$OUT/$w.json" && echo ""
done
