# Shared scene→renderer response channel: cross-scene coupling in the headless server

**Status:** design / for review — proposed owner: @robtfm (engine)
**Context:** authoritative headless server (`--orchestrated`), one engine hosting many creators' scenes
**Related:** interim crash mitigation already merged (see §5); per-scene resource analytics (separate doc, orchestrator side)

---

## 1. TL;DR

In the orchestrated headless server, **every scene shares one bounded `SceneResponse`
channel** (and, in the sidecar, one more) drained by **one consumer**. This couples
scenes that are supposed to be isolated: one scene emitting a burst of (or oversized)
responses can **delay, starve, or drop the tick results of other, well-behaved scenes**
on the same engine — head-of-line blocking plus shared-slot exhaustion.

We shipped an interim fix that removes the *fatal* symptom (a full channel used to panic
and take the **whole engine** — every co-tenant scene — down). It does **not** remove the
coupling: a greedy scene can still stall its neighbors, just not crash them.

Fully removing the coupling needs **per-scene channels + a fair drain**. That code lives in
`crates/scene_runner`, which is **shared with the desktop client**, so it must be done in a
way that leaves the desktop path unchanged (runtime-gated on the headless flag). This is the
change we'd like @robtfm to own.

---

## 2. Background: the invariant

The headless server runs **one engine process** + **one sidecar process**
(`crates/dcl_deno_ipc`). Inside the sidecar, each scene runs in its **own V8 isolate on its
own OS thread**. Scene JS is **untrusted creator code**. The orchestrator
(`sdk-multiplayer-server`) adds/removes scenes and restarts broken ones.

**Invariant we want:** one creator's scene — buggy or hostile — must not degrade or break
**other** scenes on the same engine. The V8 isolate already gives us JS-heap and global
isolation. The response channel is a place where that isolation leaks.

---

## 3. Current architecture (the data path)

A scene's per-tick result (`SceneResponse::Ok` — CRDT diffs, logs, rpc calls) travels:

```
scene thread (sidecar)
  └─ SceneResponseSender  ── shared bounded(1000) ──▶  scene_ipc_out  ──▶ [ SOCKET ] ──▶ renderer_ipc_in
       crates/dcl/src/js/engine.rs:104                 dcl_deno_ipc/main.rs:67                dcl_deno_ipc/lib.rs:269
                                                                                                    │
                                                                            RENDERER_SENDER.try_send │
                                                                                                    ▼
                                                            SceneUpdates.sender ── shared bounded(1000) ──▶ receive_scene_updates
                                                            scene_runner/src/lib.rs:82,256                  scene_runner/src/lib.rs:952 (Bevy main thread)
```

There are **two shared bounded channels** and **two single consumers**:

| Stage | Channel | Created | Shared by | Drained by |
|---|---|---|---|---|
| Sidecar | `scene_sx` bounded(1000) | `dcl_deno_ipc/src/main.rs:49` | every scene thread (`scene_sx.clone()`, `main.rs:130`) | one task `scene_ipc_out` → socket (`main.rs:67`) |
| Engine | `SceneUpdates` bounded(1000) | `scene_runner/src/lib.rs:256` (`scene_response_channel()`, `dcl/src/js/mod.rs:56`) | every scene (`scene_updates.sender.clone()`, `initialize_scene.rs:664`); routed via one `RENDERER_SENDER` thread-local (`dcl_deno_ipc/lib.rs:60`) | Bevy main thread (`receive_scene_updates`, `scene_runner/src/lib.rs:952`), bounded by `loop_end_time` (`lib.rs:1074`) |

Plus two serialization points that are shared no matter how we queue, because it is one
engine: **the single IPC socket** and **the single Bevy main-thread drain**.

---

## 4. The coupling

Because it is one FIFO queue with a single consumer, a scene can affect its neighbors three
ways:

1. **Head-of-line blocking.** Scene A's messages sit *ahead* of scene B's in the same queue.
   The consumer processes FIFO, so B's tick result waits behind A's backlog — B is delayed
   for something A did.
2. **Drain monopolization.** The Bevy consumer drains only until the frame budget expires
   (`receive_scene_updates` loops `try_recv()` until `loop_end_time`, `lib.rs:1074`). If A's
   messages fill that budget, B's aren't processed this frame at all.
3. **Shared-slot exhaustion / unfair drops.** When the 1000 slots are full, the *next* send —
   which may be innocent scene B's — is the one that fails. (Before the interim fix that
   failure was a panic; now it's a dropped frame — but still B's.)

### How a scene triggers it

Normal flow control gives each scene ~1 outstanding tick: the engine marks a scene
`in_flight` when it sends a tick and won't send the next until the response returns. **But
that gate is one-directional (engine→scene).** A scene controls its own op calls, so a scene
that calls the CRDT-send op repeatedly within a single `onUpdate` (or emits oversized CRDT /
log payloads) pushes many/large `SceneResponse`s into the shared queue in one tick, bypassing
the in-flight gate. That is the flood.

### Why it matters here specifically

In the desktop client this queue is effectively single-tenant (one local user's scenes,
cooperative). In the **headless server it is multi-tenant with untrusted code** — the exact
setting where one tenant must not be able to degrade another. Same channel, very different
threat model.

---

## 5. Interim mitigation (already merged)

`crates/dcl_deno_ipc/src/lib.rs` `renderer_ipc_in`: the engine-side `try_send(...).unwrap()`
that fired on a full channel used to panic. In the sidecar-IPC task that panic ends the IPC
loop, which trips `EXIT_ON_SIDECAR_LOSS` → `std::process::exit(1)` → **the whole engine dies,
every co-tenant scene with it**. We gated a drop-and-log path on `EXIT_ON_SIDECAR_LOSS` (set
only by the headless binary, `src/bin/headless.rs:229`):

```rust
if EXIT_ON_SIDECAR_LOSS.load(Ordering::SeqCst) {
    if let Err(e) = sender.try_send(scene_response) {
        warn!("dropping scene response: renderer channel unavailable ({e})");
    }
} else {
    sender.try_send(scene_response).unwrap(); // desktop/tests: unchanged
}
```

- **Fixes:** "one scene kills *every* scene" (no more process exit).
- **Does NOT fix:** the coupling in §4. A flood still fills the shared queue and stalls /
  drops neighbors' frames — B stutters instead of dying.
- **Note:** the sidecar-side twin (`crates/dcl/src/js/engine.rs:104`, `:163`
  `try_send(...).expect(...)`) is left as-is; it panics on the scene thread, caught by
  `catch_unwind` in `spawn_scene`, so it kills only the offending/one scene rather than the
  process. It is shared `dcl` code (native + wasm), so it can't be changed headless-only
  without a `dcl`-visible gate. Lower severity; folds into the real fix below.

---

## 6. Proposed fix: per-scene channels + fair drain

**Goal:** each scene owns its own bounded queue; the consumer serves scenes fairly. A flood
fills only the offender's queue and drops only the offender's frames; neighbors are untouched.

### Constraint

The queue and its consumer (`SceneUpdates`, `receive_scene_updates`) live in
`crates/scene_runner`, **shared with the desktop client**. The desktop path must stay
byte-for-byte. So the per-scene path is **runtime-gated on the headless flag**
(`EXIT_ON_SIDECAR_LOSS`, or a dedicated `ORCHESTRATED` bool set alongside it); when the flag
is unset, everything behaves exactly as today.

### Shape A — true per-scene channels (removes the coupling)

- Each scene owns its receiver (e.g. on `RendererSceneContext`).
- `renderer_ipc_in` routes each `SceneResponse` by `scene_id` (already carried on the message)
  to that scene's sender, replacing the single `RENDERER_SENDER` with a
  `HashMap<SceneId, Sender>` populated on `NewScene`, removed on `KillScene`.
- `receive_scene_updates` drains scenes **round-robin** within `loop_end_time`, with a
  per-scene per-frame budget so no single scene consumes the whole drain.
- A full per-scene queue drops that scene's own frames (attributed, logged).

Touch points: `SceneUpdates`, `RendererSceneContext`, `initialize_scene` (create per-scene
channel), `receive_scene_updates` (fair drain), `dcl_deno_ipc` routing. Dual code path gated
on the flag.

### Shape B — per-scene buffering in the IPC layer only (partial, native-only)

Keep the shared `SceneUpdates` channel; in `renderer_ipc_in` hold a
`HashMap<SceneId, bounded queue>` and feed the shared channel round-robin, gated on the flag.
Removes the drop-unfairness and burst head-of-line blocking without editing `scene_runner`,
but the downstream shared channel + single drain remain — so it's "fairer, not decoupled."

**Recommendation:** Shape A is the honest fix and pairs naturally with the fair drain (same
code). Shape B is the fallback if we want to stay out of `scene_runner`.

### The residual (both shapes)

The single socket and single main-thread drain are shared *throughput* points inherent to one
engine. Per-scene channels fix **queue fairness** (this doc). Bounding one scene's share of
raw throughput additionally wants **per-tick output caps** (cap CRDT/log/comms bytes & count a
scene emits per tick) — see the analytics doc, which will give us the data to set them.

---

## 7. Open questions for the engine owner

1. Prefer per-scene receivers on `RendererSceneContext`, or a `HashMap<SceneId, receiver>` on
   `SceneUpdates`? (Lifecycle: context is despawned with the scene → receiver auto-dropped,
   which is convenient.)
2. Fair-drain policy: strict round-robin, or weighted by priority
   (`context.priority`)? Per-scene per-frame message/byte budget — what default?
3. Is a dedicated `ORCHESTRATED` flag preferable to overloading `EXIT_ON_SIDECAR_LOSS` as the
   gate, given more headless-only behaviors are likely (watchdog, output caps)?
4. Should the sidecar `scene_sx` (stage 1) get the same treatment, or is stage-2 (engine)
   sufficient in practice? (Stage 1's panic is per-scene/caught; stage 2 was the fatal one.)

---

## 8. File / line index

| What | Location |
|---|---|
| Engine shared channel created | `crates/scene_runner/src/lib.rs:256` |
| `SceneUpdates` sender/receiver | `crates/scene_runner/src/lib.rs:82-83` |
| Per-scene sender clone | `crates/scene_runner/src/initialize_scene.rs:664` |
| Bevy drain loop / frame budget | `crates/scene_runner/src/lib.rs:952`, `:1074` |
| Scheduler slot gate (`scene_threads`, 16) | `crates/scene_runner/src/lib.rs:746`; `src/bin/headless.rs:107` |
| `channel(1000)` def | `crates/dcl/src/js/mod.rs:56` |
| Scene-thread send (caught panic) | `crates/dcl/src/js/engine.rs:104`, `:163` |
| Sidecar shared channel | `crates/dcl_deno_ipc/src/main.rs:49`, drain `:67` |
| Engine IPC receiver + interim fix | `crates/dcl_deno_ipc/src/lib.rs:269` |
| Headless flag set | `src/bin/headless.rs:229` (`EXIT_ON_SIDECAR_LOSS`) |
