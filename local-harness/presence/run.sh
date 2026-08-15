#!/usr/bin/env bash
# Presence-isolation end-to-end harness. Boots a local livekit dev server, an
# orchestrated (multi-tenant) headless engine holding two scenes in two rooms, and
# synthetic clients (headless client-mode instances) joining one room each. Asserts
# that each scene sees only its own room's client, that a forged cross-room bus
# message is dropped, and that the shared-context client path still sees a peer.
#
# Usage: ./run.sh          (full matrix, prints PASS/FAIL and exits non-zero on fail)
#        KEEP=1 ./run.sh   (leave processes/logs up for inspection)
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
WORK="$HERE/.work"
LOGS="$WORK/logs"
CONTENT_PORT=8100
LK_URL="ws://localhost:7880"
LK_HTTP="http://localhost:7880"

ROOM_A="harness-room-a"
ROOM_B="harness-room-b"
ROOM_C="harness-room-c"          # client-regression room
# content-derived suffix so editing game.js auto-busts the engine's on-disk content
# cache (which is keyed by hash) — a stale hash would serve old scene JS
VER=$(md5 -q "$(cd "$(dirname "$0")" && pwd)/scene/game.js" 2>/dev/null | cut -c1-10)
VER="${VER:-00000harns}"
SCENE_A="bafkharnessa${VER}aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
SCENE_B="bafkharnessb${VER}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
SCENE_C="bafkharnessc${VER}cccccccccccccccccccccccccccccc"
GAMEJS="bafkharnessg${VER}gggggggggggggggggggggggggggggg"
# NOTE: the regression observer loads scene C as a client, so when it enters the scene it
# asks the comms gatekeeper for a scene-room adapter. Online, that mints a real (empty)
# dcl.livekit.cloud room; offline, the request times out (10s, async) and is skipped.
# Either way the regression passes — peer visibility rides the LOCAL realm-comms room, not
# the scene room. `b64-`-prefixing the hash only swaps the prod gatekeeper hostname for
# `comms-gatekeeper-local`, which resolves to the same infra, so it doesn't help. Truly
# offline/hermetic would need a gatekeeper URL override pointing at a local mint.
BASEURL="http://localhost:${CONTENT_PORT}/content/contents/"

SEED_CLIENT_A=2
SEED_CLIENT_B=3
SEED_FORGER=4
SEED_PEER_C=5
SEED_OBS_C=6

HEADLESS="$REPO/target/debug/headless"
UTIL="$REPO/target/debug/harness-util"
PIDS=()

say() { printf '\033[1;36m[harness]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[harness] FATAL:\033[0m %s\n' "$*" >&2; cleanup; exit 2; }

cleanup() {
  [ "${KEEP:-0}" = "1" ] && { say "KEEP=1, leaving processes up"; return; }
  # results are already printed; silence the shell's job-termination chatter
  exec 2>/dev/null
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null; done
  pkill -f "livekit-server --dev" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

# ---- preflight ----
command -v livekit-server >/dev/null || die "livekit-server not found (brew install livekit)"
command -v python3 >/dev/null || die "python3 not found"

rm -rf "$WORK"; mkdir -p "$LOGS" "$WORK/content/contents"

say "building headless + sidecar + harness-util (debug)..."
# headless spawns the dcl_deno_ipc sidecar binary; both are required (see CLAUDE.md)
( cd "$REPO" && cargo build --package dcl_deno_ipc \
    && cargo build --features headless --bin headless \
    && cargo build -p harness-util ) \
  >"$LOGS/build.log" 2>&1 || die "build failed (see $LOGS/build.log)"

# ---- content: two scenes sharing one game.js, plus a regression scene ----
cp "$HERE/scene/game.js" "$WORK/content/contents/$GAMEJS"
write_entity() { # <hash> <base-parcel>
  cat >"$WORK/content/contents/$1" <<JSON
{
  "id": "$1",
  "pointers": ["$2"],
  "content": [{"file": "game.js", "hash": "$GAMEJS"}],
  "metadata": {
    "main": "game.js",
    "scene": {"base": "$2", "parcels": ["$2"]},
    "runtimeVersion": "7"
  }
}
JSON
}
write_entity "$SCENE_A" "0,0"
write_entity "$SCENE_B" "0,0"
write_entity "$SCENE_C" "0,0"

# ---- realm abouts ----
mkdir -p "$WORK/content/server" "$WORK/content/clientA" "$WORK/content/clientB" \
         "$WORK/content/peerC" "$WORK/content/obsC"
write_about() { # <dir> <fixed-adapter|-> <scenes-urn-json>
  local adapter_json="null"
  [ "$2" != "-" ] && adapter_json="\"$2\""
  cat >"$WORK/content/$1/about" <<JSON
{
  "content": {"healthy": true, "publicUrl": "http://localhost:${CONTENT_PORT}/content/contents/"},
  "lambdas": {"healthy": true, "publicUrl": "http://localhost:${CONTENT_PORT}/content/lambdas/"},
  "comms": {"healthy": true, "protocol": "v3", "fixedAdapter": $adapter_json, "adapter": null},
  "configurations": {"realmName": "harness-$1", "scenesUrn": $3, "map": {"minimapEnabled": false, "sizes": []}}
}
JSON
}

# ---- livekit dev server ----
pkill -f "livekit-server --dev" 2>/dev/null; sleep 0.3
say "starting livekit-server --dev..."
livekit-server --dev >"$LOGS/livekit.log" 2>&1 &
PIDS+=($!)
for i in $(seq 1 40); do
  curl -sf "$LK_HTTP" >/dev/null 2>&1 && break
  sleep 0.25
  [ "$i" = 40 ] && die "livekit did not come up (see $LOGS/livekit.log)"
done

# ---- content server ----
say "starting content server on :$CONTENT_PORT..."
# serve from $WORK: all URL paths are rooted at /content/… so the dir tree matches
python3 "$HERE/serve.py" "$CONTENT_PORT" "$WORK" >"$LOGS/serve.log" 2>&1 &
PIDS+=($!)
for i in $(seq 1 40); do
  curl -sf "http://localhost:${CONTENT_PORT}/content/contents/$GAMEJS" >/dev/null 2>&1 && break
  sleep 0.25
  [ "$i" = 40 ] && die "content server did not come up"
done

mint() { "$UTIL" token --room "$1" --identity "$2"; }
mint_seed() { "$UTIL" token --room "$1" --seed "$2"; }
addr() { "$UTIL" address --seed "$1"; }

ADDR_A=$(addr $SEED_CLIENT_A); ADDR_B=$(addr $SEED_CLIENT_B)
ADDR_PEER_C=$(addr $SEED_PEER_C)
say "client A address: $ADDR_A"
say "client B address: $ADDR_B"

# server tokens (identity 'authoritative-server' — clients address the server by it)
TOK_SRV_A=$(mint "$ROOM_A" "authoritative-server")
TOK_SRV_B=$(mint "$ROOM_B" "authoritative-server")
# client tokens (identity == wallet address, so the server maps them correctly)
TOK_CLI_A=$(mint_seed "$ROOM_A" $SEED_CLIENT_A)
TOK_CLI_B=$(mint_seed "$ROOM_B" $SEED_CLIENT_B)
# regression room tokens
TOK_PEER_C=$(mint_seed "$ROOM_C" $SEED_PEER_C)
TOK_OBS_C=$(mint_seed "$ROOM_C" $SEED_OBS_C)
# observer scene-room token: a separate LOCAL room the observer joins as its scene room via
# DCL_SCENE_ROOM_ADAPTER, bypassing the comms gatekeeper so the test never touches prod.
# (distinct room from ROOM_C so the same identity isn't joined to one room twice)
ROOM_SCENE_C="harness-scene-c"
TOK_SCENE_C=$(mint_seed "$ROOM_SCENE_C" $SEED_OBS_C)

write_about server - "[]"
write_about clientA "livekit:${LK_URL}?access_token=${TOK_CLI_A}" "[]"
write_about clientB "livekit:${LK_URL}?access_token=${TOK_CLI_B}" "[]"
write_about peerC   "livekit:${LK_URL}?access_token=${TOK_PEER_C}" "[]"
# observer loads the regression scene as a world scene so it can read its shared-context roster
write_about obsC "livekit:${LK_URL}?access_token=${TOK_OBS_C}" \
  "[\"urn:decentraland:entity:${SCENE_C}?=&baseUrl=${BASEURL}\"]"

# ================= scenario 1: orchestrated isolation =================
say "starting orchestrated server (2 scenes / 2 rooms)..."
SRV_IN="$WORK/server.in"; mkfifo "$SRV_IN"
"$HEADLESS" --orchestrated --realm "http://localhost:${CONTENT_PORT}/content/server" \
  --tick-hz 30 <"$SRV_IN" >"$LOGS/server.log" 2>&1 &
SRV_PID=$!; PIDS+=($SRV_PID)
exec 8>"$SRV_IN"   # hold the write end open
sleep 3
printf '%s\n' "{\"type\":\"add-scene\",\"sceneId\":\"$SCENE_A\",\"urn\":\"urn:decentraland:entity:${SCENE_A}?=&baseUrl=${BASEURL}\",\"adapter\":\"livekit:${LK_URL}?access_token=${TOK_SRV_A}\"}" >&8
printf '%s\n' "{\"type\":\"add-scene\",\"sceneId\":\"$SCENE_B\",\"urn\":\"urn:decentraland:entity:${SCENE_B}?=&baseUrl=${BASEURL}\",\"adapter\":\"livekit:${LK_URL}?access_token=${TOK_SRV_B}\"}" >&8

# wait for both scenes to start ticking (ctl emits {"type":"scene-live",...})
say "waiting for scenes to start..."
for i in $(seq 1 120); do
  grep -q "scene-live\".*\"$SCENE_A\"\|\"$SCENE_A\".*scene-live" "$LOGS/server.log" 2>/dev/null && \
  grep -q "scene-live\".*\"$SCENE_B\"\|\"$SCENE_B\".*scene-live" "$LOGS/server.log" 2>/dev/null && break
  sleep 0.5
  [ "$i" = 120 ] && say "WARN: scenes may not have started (continuing)"
done

say "starting synthetic clients..."
"$HEADLESS" --realm "http://localhost:${CONTENT_PORT}/content/clientA" \
  --wallet-seed $SEED_CLIENT_A \
  >"$LOGS/clientA.log" 2>&1 & PIDS+=($!)
"$HEADLESS" --realm "http://localhost:${CONTENT_PORT}/content/clientB" \
  --wallet-seed $SEED_CLIENT_B \
  >"$LOGS/clientB.log" 2>&1 & PIDS+=($!)

say "letting presence propagate (12s)..."
sleep 12

# legitimate bus (positive control): client A's room, correct scene id
say "sending legit bus message into room A..."
"$UTIL" bus --url "$LK_URL" --room "$ROOM_A" --seed $SEED_FORGER \
  --scene-hash "$SCENE_A" --message "LEGIT_A" >"$LOGS/bus_legit.log" 2>&1 || \
  say "WARN: legit bus send failed"
# forged bus: injected via room B but declaring scene A's id — must be dropped
say "sending forged bus message (room B, declares scene A)..."
"$UTIL" bus --url "$LK_URL" --room "$ROOM_B" --seed $SEED_FORGER \
  --scene-hash "$SCENE_A" --message "FORGERY" >"$LOGS/bus_forge.log" 2>&1 || \
  say "WARN: forged bus send failed"
sleep 4

# ================= scenario 2: client-regression =================
say "starting client-regression (observer + peer in one room)..."
"$HEADLESS" --realm "http://localhost:${CONTENT_PORT}/content/peerC" \
  --wallet-seed $SEED_PEER_C \
  >"$LOGS/peerC.log" 2>&1 & PIDS+=($!)
# the observer loads scene C; DCL_SCENE_ROOM_ADAPTER makes its scene-room connection a
# local livekit room instead of a gatekeeper-minted prod one, so the test stays offline
DCL_SCENE_ROOM_ADAPTER="livekit:${LK_URL}?access_token=${TOK_SCENE_C}" \
"$HEADLESS" --realm "http://localhost:${CONTENT_PORT}/content/obsC" \
  --wallet-seed $SEED_OBS_C \
  >"$LOGS/obsC.log" 2>&1 & PIDS+=($!)
sleep 12

exec 8>&-   # close stdin → server exits
sleep 1

# ================= assertions =================
say "evaluating..."
FAIL=0
check() { # <description> <0-for-pass>
  if [ "$2" = 0 ]; then printf '  \033[1;32mPASS\033[0m %s\n' "$1"
  else printf '  \033[1;31mFAIL\033[0m %s\n' "$1"; FAIL=1; fi
}

# Grep the @scene-log lines the ORCHESTRATED server emitted for one scene hash. The
# scene's HARNESS console.log payloads (identity rosters, event data) carry the client
# wallet address, so presence/absence of an address in a scene's log is the isolation
# signal — robust to the doubly-escaped JSON in the msg field.
srv_scene_has() { # <scene-hash> <needle> -> 0 if present
  grep -a "@scene-log {\"scene\":\"$1\"" "$LOGS/server.log" | grep -q "$2"
}
# A synthetic client runs its scene in-process, so its scene logs land in its own stdout
# log as `LOG "HARNESS|..."` lines (not @scene-log ctl frames).
client_scene_has() { # <client-log> <needle> -> 0 if present
  grep -a 'HARNESS|' "$1" | grep -q "$2"
}
neg() { [ "$1" -ne 0 ] && echo 0 || echo 1; }  # invert an exit code for "must be absent"

# 1. presence isolation — each scene's CRDT/event stream carries only its own client
srv_scene_has "$SCENE_A" "$ADDR_A"; check "scene A sees client A"          $?
srv_scene_has "$SCENE_A" "$ADDR_B"; check "scene A never sees client B"    $(neg $?)
srv_scene_has "$SCENE_B" "$ADDR_B"; check "scene B sees client B"          $?
srv_scene_has "$SCENE_B" "$ADDR_A"; check "scene B never sees client A"    $(neg $?)

# 2. onPlayerConnected isolation — fired for own client only
grep -a "@scene-log {\"scene\":\"$SCENE_A\"" "$LOGS/server.log" \
  | grep "playerConnected" | grep -q "$ADDR_A"; check "scene A playerConnected fired for client A" $?
grep -a "@scene-log {\"scene\":\"$SCENE_A\"" "$LOGS/server.log" \
  | grep "playerConnected" | grep -q "$ADDR_B"; check "scene A playerConnected never for client B" $(neg $?)

# 3. bus forgery — legit (room A, scene A id) delivered; forged (room B, scene A id) dropped
srv_scene_has "$SCENE_A" "LEGIT_A"; check "scene A received legitimate bus message" $?
srv_scene_has "$SCENE_A" "FORGERY"; check "scene A dropped forged cross-room bus message" $(neg $?)

# 4. client-regression — non-orchestrated observer's scene sees its room peer (shared ctx)
client_scene_has "$LOGS/obsC.log" "$ADDR_PEER_C"; check "regression: observer's scene sees peer (shared ctx)" $?

# 5. getConnectedPlayers RPC — resolves (an inline await here used to self-deadlock the
# scene: RpcCalls flush only on crdtSendToRenderer) and is room-scoped like the CRDT view
srv_has_line() { # <scene-hash> <line-kind> <needle> -> 0 if a line of that kind matches
  grep -a "@scene-log {\"scene\":\"$1\"" "$LOGS/server.log" | grep "$2" | grep -q "$3"
}
srv_has_line "$SCENE_A" "connected-players" "$ADDR_A"; check "getConnectedPlayers on scene A resolved and lists client A" $?
srv_has_line "$SCENE_A" "connected-players" "$ADDR_B"; check "getConnectedPlayers on scene A never lists client B"       $(neg $?)
srv_has_line "$SCENE_B" "connected-players" "$ADDR_B"; check "getConnectedPlayers on scene B resolved and lists client B" $?
srv_has_line "$SCENE_B" "connected-players" "$ADDR_A"; check "getConnectedPlayers on scene B never lists client A"       $(neg $?)

echo
if [ $FAIL = 0 ]; then printf '\033[1;32m==== ALL PASS ====\033[0m\n'
else printf '\033[1;31m==== FAILURES ====\033[0m  (logs in %s)\n' "$LOGS"; fi
exit $FAIL
