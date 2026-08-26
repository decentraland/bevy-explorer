# Presence-isolation harness

End-to-end proof that the `proto/crdt-contexts` presence-isolation design keeps each
scene's player view scoped to its own room on a multi-tenant (orchestrated) headless
server, and that the client (shared-context) path is unchanged.

## Run

```bash
./run.sh            # full matrix, prints PASS/FAIL, exits non-zero on any failure
KEEP=1 ./run.sh     # leave livekit/server/clients up and logs in .work/logs for poking
```

Requires `livekit-server` on PATH (`brew install livekit`) and `python3`. Builds the
engine itself, so the first run is slow; artifacts are cached after.

Also needs a reachable **Pulse** server, because client avatar state rides Pulse rather
than LiveKit — without one the two regression peers never see each other's positions.
Defaults to the shared dev endpoint `pulse-server.decentraland.zone:7777`, resolved once
and pinned so both peers land on the same instance; each run therefore puts two
short-lived synthetic players on that dev service (in realm `harness-c`). Override with
`PULSE_SERVER=host:port`. A *local* server only works on Linux: bevy's native ENet client
cannot reach one running on macOS (the mac `libenet.dylib` drops the CONNECT before it
reaches the C# layer). The orchestrated server never joins Pulse at all — it ingests its
clients' avatar state from their LiveKit scene rooms, addressed to `authoritative-server`.

## What it stands up

- **`livekit-server --dev`** — local SFU (devkey/secret, rooms auto-create on join).
- **`serve.py`** — static content server: serves the two scene entity definitions, the
  shared `game.js`, and one realm `about` per engine instance. Answers the
  `entities/active` pointer query with `[]` so synthetic clients load no scene of their own.
- **Orchestrated server** — `headless --orchestrated`, fed two `add-scene` commands on
  stdin, each with a pre-minted `livekit:` adapter for a distinct room. One
  `GlobalCrdtState` context per room.
- **Synthetic clients** — `headless` in client mode (`--realm-comms --no-scene-room
  --wallet-seed N`). A client-mode headless *is* a synthetic player: it broadcasts
  position/profile into whatever room its realm's `about.comms.fixedAdapter` points at.
  Client A joins room A, client B joins room B.
- **`harness-util`** — small Rust bin (workspace member): derives the deterministic guest
  address for a seed, mints livekit dev tokens, and publishes a forged/legit rfc4 Scene
  bus packet into a room.

## The scene (`scene/game.js`)

Raw SDK7 CRDT ops — no `@dcl/sdk` toolchain. It parses `PLAYER_IDENTITY_DATA`
(component 1089) out of the renderer→scene CRDT stream to build the scene's authoritative
roster of players, subscribes to `playerConnected`/`playerDisconnected`/`comms`, and logs
everything as greppable `HARNESS|<kind>|<json>` lines. The engine tags each scene's console
output with its hash (`@scene-log {"scene":"<hash>",...}`), so the harness attributes every
line to the right scene.

## Assertions (all must pass)

1. Scene A's CRDT/events carry client A and **never** client B; scene B the mirror.
2. `onPlayerConnected` fires for the own-room client only.
3. A legit bus message (published into room A declaring scene A's id) is delivered; a
   forged one (published into room B but declaring scene A's id) is dropped by the
   per-context room-hash guard.
4. Client-regression: a non-orchestrated observer sharing a room with a peer still sees
   that peer through the single shared context — roster and profile over LiveKit, position
   over Pulse.
5. `remove-scene` tears a scene down and takes its players with it, rather than stranding
   them in a context nothing hosts. Client B is still connected when scene B is removed, so
   its player entity is genuinely live until the teardown reaches it. The listener side has
   to let go too: its announced AoI drops back to scene A's realm alone. Scene A is the
   control — it was never removed. Note the players go via `remove_context_players` (the
   crdt context is reaped once the scene releases it), *not* via the transport-emptied path
   in `despawn_players`; asserting on the wrong one of those two looks exactly like a leak.

Transform assertions match within one Pulse quantization step (0.0625m) rather than
exactly. That tolerance is also a signal: a scene-A/B transform arrives over LiveKit as raw
floats and reads exactly `[8,0,8]`, while scene C's arrives over Pulse and reads `[8.031,
0, 8.031]` — the centre of its quantization box. Both paths are live and distinguishable.

## Notes

- Scene/js content hashes embed `md5(game.js)` so editing the scene auto-busts the
  engine's on-disk content cache.
- Don't set `RUST_LOG` (the script refuses if you do). It replaces the engine's whole log
  filter, and the clients' `HARNESS|` scene lines are tracing INFO — muting them looks
  exactly like the observer failing to see its peer. The server's scene lines are
  `@scene-log` stdout frames and survive any filter, so only the client-side assertions
  break, which makes the symptom especially misleading.
- `getConnectedPlayers()` is currently disabled in the scene (`WANT_CONNECTED_PLAYERS`):
  that async RPC does not resolve on the orchestrated server (the scene then wedges and is
  marked broken). The CRDT `PLAYER_IDENTITY_DATA` roster is the stronger isolation signal
  and is what the matrix asserts on. See the memory note for follow-up.
- Nothing here is committed; it lives under `local-harness/` until the isolation change
  itself is ready to land.
