# Headless multi-scene bevy — cross-scene isolation (Fable pass)

Scope: can one untrusted scene steal/corrupt a co-tenant scene's data inside a single
bevy engine + single `dcl_deno_ipc` sidecar, with focus on the two classes the operator
flagged: **JS `Proxy`/realm cross-scene access** and **Storage**. Cross-referenced against
the hammurabi-headless hardening commits. Every claim is file:line-cited.

## Verdict on the Proxy concern

**Structurally blocked, and stronger than hammurabi's model.** Each scene gets its own V8
**isolate**, not a shared isolate with per-scene contexts:

- `dcl_deno_ipc/src/main.rs:125` → `dcl_deno::spawn_scene` → `crates/dcl_deno/src/lib.rs:49`
  spawns a **fresh OS thread per scene**, each calling `create_runtime` →
  `JsRuntime::new` (`crates/dcl_deno/src/js/mod.rs:150`). One `JsRuntime` == one V8 isolate.
- A `Proxy` (or prototype/global tamper) in scene A's isolate **cannot reference an object
  in scene B's isolate** — separate heaps, no shared object graph. The classic
  cross-realm/Proxy escape (multiple scenes in one runtime) is not reachable.
- Hammurabi's `isUint8Array`-non-writable fix (commit `2a8f17e`) guarded against a scene
  tampering marshalling globals to forge host-side bytes. Here that is **self-only**: even
  if scene A rewrites its own prototypes, it corrupts its own isolate's marshalling; the
  forged bytes go into scene A's own ops. The host op layer reads scene identity from
  **host-controlled `OpState`** (`CrdtContext.scene_id` / `player_identity`), never from a
  scene-supplied value — so tampering can't redirect an op at scene B.

### Invariant that MUST NOT regress
Keep **one isolate per scene**. A memory optimization that collapses scenes into one
isolate with multiple V8 `Context`s reopens the entire Proxy/cross-realm escape class.
This is the single load-bearing JS-isolation boundary.

## Verdict on Storage

**Isolated today, but the isolation is incidental on pid-uniqueness, not asserted.**

Two independent separations exist:
1. **On-disk folder** = `SHA-256(storage_root)` base64url'd (`mod.rs:69-75`), joined under
   `LocalStorage/`. Because it is a **hash**, a malicious `storage_root` full of `../`
   cannot traverse out of `LocalStorage/` — path traversal is neutralized. Each scene's
   webstorage extension is initialized with its own folder at isolate-build time
   (`mod.rs:76`); scene JS has no op to change it.
2. **In-folder key prefix** = `address` = the **player wallet address**
   (`local_storage.rs:19-25, 65-96`), applied to every get/set/remove/iterate.

`storage_root` resolves to `portable.pid` for portable scenes
(`initialize_scene.rs:379-385`), and in orchestrated mode `pid = urn` supplied by the
**trusted parent** over the stdin control channel (`src/bin/headless.rs:593-597`,
`super_user: false`). The urn contains the scene's content hash, so it is unique per scene
and not scene-settable → distinct folder per scene → **Storage is isolated**.

### The trap to guard against (this is where a regression bites)
- On the **server, every scene runs as the same identity**, so the `address` key-prefix is
  **identical across all co-tenants**. It provides **zero** cross-scene separation. The only
  thing separating scenes is the per-scene **folder** (hashed `storage_root`).
- Therefore: **if two scenes ever share a `storage_root`** (a pid collision, an
  engine-wide "shared data dir" optimization, or two scenes of the same world/urn), they
  land in the **same LocalStorage DB with the same key prefix** → full cross-scene
  read/write/clear. Nothing asserts pid-uniqueness.
- **Recommendation:** assert `storage_root` is unique per live scene in orchestrated mode
  (e.g. reject an add-scene whose urn/pid is already active), and add a comment at
  `initialize_scene.rs:379` and `local_storage.rs` stating the address-prefix is a
  client multi-user feature and is NOT a cross-scene boundary on the server.
- Hammurabi's own storage work (`ffdf82b`, `b597bf8`, `34189e3`, `6666d21`) scoped
  storage delegation to `world/sceneId/parcel`, root-signed, HTTPS-only, no silent rebind.
  If/when bevy grows a **world-storage client** (today `op_read_file`/localStorage are the
  only storage and there is no remote world-storage signer — headless treats
  storage-delegation as a no-op, `headless.rs:677-679`), it must adopt the same
  scene-scoped, non-rebindable delegation. **Re-audit when storage lands.**

### Secondary storage vector (low, note it)
`op_portable_spawn` (`crates/dcl/src/js/portables.rs:10-22`) lets scene JS name an
**arbitrary urn**, which becomes the spawned portable's `storage_root`. Exfiltration is
still blocked — the spawned portable runs the code fetched from that urn's own baseUrl, so
an attacker can't run *their* code inside a victim's storage folder — but it is gated behind
`PermissionType::SpawnPortable` (`crates/restricted_actions/src/lib.rs:770`) which
**fails-closed** on a non-interactive server only if its default is `Ask`/`Deny`. See the
permission-default finding below.

## Confirmed findings (adversarially verified)

| # | Finding | Cross-scene? | Severity | Fix |
|---|---------|--------------|----------|-----|
| S1 | **`op_read_file` is an unfiltered arbitrary-URL fetch (SSRF).** When `filename` misses the scene's content-map, `IpfsType::url_target` falls through to `url::Url::try_from(file_path)` and returns it as a direct URL (`crates/ipfs/src/ipfs_path.rs:135-141`); `IpfsIo::read` does `client.get(&remote)` with **no scheme/host check** (`crates/ipfs/src/lib.rs:1569,1619`). A scene calls `op_read_file('http://169.254.169.254/latest/meta-data/...')` and gets the body — from the shared host's network vantage. Unlike `op_signed_fetch_headers` which guards scheme/loopback (`crates/dcl/src/js/fetch.rs:47-51`), this has no filter. | Shared-host | **High** | Drop the `url::Url::try_from` fallback for scene reads (content-map miss → error), or enforce the fetch.rs allowlist before `client.get`. |
| S2 | **Permission defaults on a non-interactive server.** `Permission::check` routes `Ask` to `manager.request(...)` which never resolves without a UI responder (`crates/scene_runner/src/permissions.rs:103-160`) → fails closed. But any permission whose **config default is `Allow`** is auto-granted to every tenant (e.g. `SpawnPortable` → repeat-spawn = cross-tenant resource-exhaustion DoS in the shared process). Default `PermissionConfig` map not yet read. | Cross-tenant (DoS) | **Med** | Verify default map; force untrusted-scene defaults to `Deny` in orchestrated mode. |
| S3 | **Shared OS process = kernel isolation is gone (the core architectural trade).** Hammurabi ran one process per scene, so a native memory-safety bug / panic / OOM was kernel-contained. Here ~15 scenes share one engine + one sidecar (`main.rs:124-138`). V8 isolates protect the JS heap but **not** native memory: a bug in V8/deno or a native parser (gltf/crdt/protobuf) on attacker bytes reads/corrupts all co-tenant memory (including other scenes' room tokens). Also: **engine-side per-scene Bevy systems are not `catch_unwind`-isolated** — a scene crafting data that trips a `.unwrap()`/panic in a shared system (the GLTF-reflect and `transform_and_parent:324` panics we already hit) aborts the whole `App` and kills every co-tenant. (Scene *JS-thread* panics ARE caught — `dcl_deno/src/lib.rs:53`.) | Yes | **High** (blast-radius amplifier) | Keep image/audio decode off; fuzz gltf/crdt/protobuf; patch V8; cap/segregate tenants per process; consider wrapping per-scene engine systems so one scene's panic can't abort the shared schedule. |
| S4 | **Room `access_token` handed to the scene's own JS.** `RealmInfo.room` = full `livekit:wss://host?access_token=T` (`crates/scene_runner/src/lib.rs:825`), returned verbatim by `op_realm_information` (`crates/dcl/src/js/runtime.rs:102-121`). Self-only (keyed by `context.hash`) — scene A never sees B's. But a scene can exfiltrate **its own** authoritative-server token and reconnect as `authoritative-server` from anywhere. | Self-only | **Med** | Redact the token from `RealmInfo.room` in server mode — the engine connects the transport itself (`headless.rs:546`); scene JS never needs the credential. |
| S5 | **All scenes sign with one shared guest wallet** (`handle_sign_request`, `restricted_actions/src/lib.rs:1887-1913`; guest asserted `headless.rs:288-295`). Tenant scenes are indistinguishable to signed-fetch endpoints; one scene can get the server's signature over content of its choosing. Low — it's a throwaway guest, no authoritative key reachable, non-livekit adapters refused (`headless.rs:539-545`). | Attribution | **Low** | Optional: per-scene deterministic guest sub-identity. |

## Defenses that HOLD (keep them)

- **Per-scene CRDT store** — every op keys off the engine-set `CrdtContext.scene_id`/`hash`;
  no op takes an attacker-supplied scene id. Scene A cannot read B's store.
- **Messagebus** routes by `scene.scene_id` to senders keyed by the subscriber's own
  `context.hash` (`crates/comms/src/global_crdt.rs:876-922`, `crates/dcl/src/js/events.rs:66-98`).
  A scene cannot subscribe under another scene's hash.
- **Global CRDT** (nearby-player positions/identity) is world-global by design — meant to be
  visible to all scenes; not another scene's private state.
- **`op_read_file` path traversal is NOT possible** — `normalize_path` only swaps
  backslashes (`ipfs_path.rs:612-614`); resolution is a HashMap lookup on the scene's own
  collection keyed by engine-assigned hash. `../foo` misses and errors.
- **cross-scene `killPortable` is blocked** — the guard requires
  `src.parent_scene.is_some()` (`restricted_actions/src/lib.rs:968`) and orchestrated
  top-level scenes are `parent_scene: None` (`headless.rs:595`). Keep that; add a regression
  test.

## Priority

**Before untrusted multi-tenant:** S1 (SSRF), S2 (permission defaults — incl. **disable
`SpawnPortable` in server mode**), and S3 (panic isolation — the top risk, plan below). Then
S4 (redact token). Assert per-scene `storage_root` uniqueness. Keep one-isolate-per-scene and
`parent_scene: None` under regression tests.

---

# S3 deep-dive — one scene's panic kills the whole engine

## Confirmed failure path
`app.run()` (`headless.rs:350`) drives `ScheduleRunnerPlugin::run_loop(ZERO)`. The
multi-threaded executor wraps each system in `catch_unwind`
(`bevy_ecs/.../executor/multi_threaded.rs:627`) **but** stores the payload and
`resume_unwind`s it on the main thread after the schedule (line 253-255) → unwinds through
`app.run()` → process exits. `log_panics::init()` only logs; it does not stop the unwind.
Release does **not** set `panic="abort"`, so the unwind is real (not an immediate abort), but
with no `catch_unwind` in our runner the process still dies. The orchestrator then treats it as
an engine crash: **all** scenes drop, bounded engine respawn, re-add every scene
(`bevy-engine-process.ts:240-248`). Blast radius = the whole engine.

Key distinction that makes the fix tractable: in Bevy 0.16 a **fallible** system that returns
`Err` is routed to a configurable handler (`multi_threaded.rs:638`), **not** a panic — and a
`Result::Err`/`?` bail is a clean return with world access released, so the World stays
consistent. Only raw panics (`.unwrap()`, out-of-bounds index) abort. So the fix is: stop
panicking on scene data.

## Confirmed scene-reachable panic sites (main-thread = process-fatal)
Ranked by ease of triggering. All consume already-parsed scene data (the byte readers
themselves are safe).

| # | Site | Trigger |
|---|------|---------|
| 1 | `update_scene/pointer_results.rs:504-512, 568-576` — `posns[indices[0]]`, `uvs[indices[0]]` | A GLB collider with index values ≥ vertex count (or fewer UVs than verts); fires when a pointer ray hits it. |
| 2 | `update_world/mesh_collider.rs:247, 253` — `scaled(...).unwrap()` | **Scale a collider entity to 0 (or NaN).** One transform. Trivial. |
| 3 | `update_world/mesh_collider.rs:1029` — `convex_hull(...).unwrap()` | Cylinder `MeshCollider` with radius 0 / NaN. |
| 4 | `update_world/gltf_container.rs:1179-1205` — POSITION `.unwrap()` / `panic!("no positions")`; out-of-range trimesh indices into parry | A GLB that loads OK but whose collider mesh lacks POSITION, or carries out-of-range indices. |
| 5 | `update_world/material.rs:793-795` — `h.path().unwrap()` / `IpfsPath...unwrap().unwrap()` | GLTF with embedded/data-URI textures, reported back to the scene. |
| 6 | `dcl_component/src/transform_and_parent.rs:163-188` (+ peer paths `comms/src/global_crdt.rs:517-568, 625, 700-704`) — **non-finite translation passed through unchecked** (rotation & zero-scale ARE handled, translation is not) | Scene or remote peer sends NaN/inf position → propagates into physics collider isometries. This is hammurabi's "non-finite transforms dropped," missing here. |
| 7 (lower) | `pointer_results.rs:508/572` `position.unwrap()`; `billboard.rs:117` parent `GlobalTransform` unwrap; `mesh_collider.rs:781` `panic!` on parry result; `mesh_collider.rs:1042` mesh-asset unwrap (load race) | Invariant/timing dependent; cheap to guard. |

**Not fatal / already safe:** entity-def JSON unwraps (`ipfs/src/lib.rs:1088,1105`) run on
`IoTaskPool` → unwind only the load task, scene fails to load, process survives.
`dcl_assert!`/`debug_panic!` are no-ops in release. CRDT reader (`reader.rs`), CRDT message
loop (`interface/mod.rs:361-385`), LWW, and peer `Packet::decode` are all bounds/`Result`-safe.

## Mitigation plan (layered)

**Layer 1 — kill the panic sites (the real fix).** Convert each site above to a *per-entity*
graceful skip (`let Ok(x) = … else { warn!(); continue }`, `.get()`, `unwrap_or`, `is_finite`
guard). Per-entity, not whole-system-fallible, because these systems iterate entities across
all scenes — skip the offending entity, keep the rest. Note: sites 1-7 are in systems the
**real GPU client also runs**, so these edits harden prod too (panic → skip); behavior for
valid data is unchanged. Bounded, mechanical, ~6 core + a few lower.

**Layer 2 — global safety net.** `GLOBAL_ERROR_HANDLER.set(warn)`
(`bevy_ecs::error`) at startup — default is `panic` (`handler.rs:124`); `warn` makes any
fallible-system `Err`, command, or observer error log-and-continue. Complements Layer 1 (does
**not** catch raw panics, so it is a net, not a substitute). Headless-only; does not touch prod.

**Layer 3 — process containment for the residual** (panics we can't convert: inside `gltf`
crate, parry internals, Bevy archetype ops, V8/deno native):
- Keep image/audio decode OFF (already off).
- Add the missing hammurabi comms bounds: **per-peer inbound rate limit + message size cap**
  in the packet-decode dispatch (`livekit/participant/plugin.rs`, `websocket_room.rs`) — today
  a peer can flood large/frequent packets (CPU/mem DoS).
- **Cap tenants per engine** (K engines × M scenes) so a hard abort kills M, not all.
- **Segregate untrusted/unknown deployers** into their own engine (1 tenant → only self-kills).
- Orchestrator: quarantine a scene whose add correlates with repeated engine aborts (today the
  per-scene budget only applies to graceful `scene-broken`, not engine aborts).

**Layer 4 — regression prevention.** A test that runs a hostile scene (zero-scale collider,
OOB-index GLB, NaN transform) next to a healthy scene and asserts the healthy scene keeps
ticking. Optionally a fuzz target over crafted GLB/collider params with `panic="abort"`.

## Also (per operator instruction)
Disable `SpawnPortable` for orchestrated/server-mode scenes (defense against the S2
cross-tenant spawn DoS) — force its permission to `Deny` when `orchestrated`.
