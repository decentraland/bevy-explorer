# Headless bevy as the hammurabi replacement — implementation & towerofmadness test plan

## STORAGE DELEGATION STATUS (2026-07-15)

World-storage signing is implemented (§5's "storage delegation for production worlds" is no longer out of scope). `crates/wallet/src/delegation.rs` consumes hammurabi's base64 envelope; `handle_sign_request` signs `https://storage.decentraland.{org,zone}` scene requests with the delegation's ephemeral key + `x-authoritative-scope` + authoritative metadata when the scene has one. Delivery: `--storage-delegation <b64>` / env `PROCESS_STORAGE_DELEGATION` (single-scene; bound to the claim's SceneId), or orchestrated `add-scene.storageDelegation` / `storage-delegation-response` (per scene; process-level values are refused). The engine emits `@bevy-ctl {"type":"storage-delegation-request","scene":…}` 5 min before expiry (30 s throttle). Without a delegation, storage degrades to guest signing (prod answers 400/401; scenes catch it). Also fixed bin-side: default-Allow Fetch/Websocket + permission-queue drain (scene fetch/WS promises hung forever headless), `ActivePlayerComponent` on the fake player (movePlayerTo-with-duration hang), AvatarBase/AvatarEquippedData replication (§2b.7 done). Build now requires the `headless` feature:
```
cargo build --release -p dcl_deno_ipc && cargo build --release --bin headless --no-default-features --features livekit,headless
```
Verified against 7 production worlds (cleantheclub/skychaser/towerofmadness/boedo/flagtag/fastlane/kickoff .dcl.eth): all run their server branch headless for 300 s, 0 broken/panics, ~24.6 Hz (30 Hz target), 206 MB–1.05 GB RSS (engine+sidecar; heavy worlds dominated by resident GLTF textures — see §2c). Harness: `bench/run_worlds.sh` + `bench/analyze.py`.

## IMPLEMENTATION STATUS (2026-07-14)

A working headless server binary now exists and was verified end-to-end against a local towerofmadness preview.

**Done & verified:**
- `src/bin/headless.rs` — render-free binary (no RenderPlugin/window/GPU/winit), builds with `--no-default-features` (+`--features livekit` for comms). Runs SDK7 scenes via the `TestPlugins`-equivalent plugin set + real IPFS/preview realm, fake player, guest wallet, supervisor. Registers the GLTF-scene reflect types by hand (RenderPlugin normally does this) — `register_gltf_scene_types()`.
- `isServer=true` plumbing: `IsServer` resource (`common::structs`) → `CrdtContext.is_server` (serde) → sidecar → new **synchronous** `op_is_server` (`crates/dcl/src/js/engine.rs`, wrapper `crates/dcl_deno/src/js/op_wrappers/engine.rs`) → `EngineApi.js` feature-detected `!!(op_is_server && op_is_server())`. Sidecar rebuilt.
- Comms server-mode: `send_scene_messages` keeps the scene's recipient when `IsServer` (no self-addressing) + per-scene room match (`restricted_actions`); `broadcast_position` suppressed when `IsServer` (no ghost avatar); `connect_scene_room` uses the **server** gatekeeper endpoint (`/get-server-scene-adapter`) with hammurabi's handshake metadata when `IsServer` (`crates/comms/src/lib.rs`).
- **Stage A GREEN:** `[Game] Running as SERVER` (never CLIENT), `[Server] Ready`, `[Server] New round` / `Tower created`, steady ~27 Hz ticks, 0 panics.
- **Stage B server-side GREEN:** joins the LiveKit scene room as participant identity **`authoritative-server`**, stays connected, keeps ticking, no duplicate-identity kick.

**Remaining (environment-blocked here):** a live remote client's bidirectional view (time-sync round trip, real display-name presence via an `AVATAR_BASE` replication system — `PLAYER_IDENTITY_DATA` already flows via comms). Needs a GPU explorer client with sole ownership of the room. Run command that works today:
```
# preview: cd towerofmadness && sdk-commands start --port 8000 --no-browser --no-client   (kill its auto-spawned hammurabi child to free the room)
# build:   cargo build --release -p dcl_deno_ipc && cargo build --release --bin headless --no-default-features --features livekit
# run:     ./target/release/headless --realm http://localhost:8000 --preview --server-mode --location 0,0
```

---

Companion to `/Users/boedo/Documents/Decentraland/bevy-explorer/HEADLESS_PLAN.md` (referenced as **PLAN**). PLAN establishes feasibility, the binary spec (§4), milestones M0–M4, and the benchmark. This document narrows it to: the exact engine changes to run a **server-role** scene, and a three-stage test against the real scene `/Users/boedo/Documents/Decentraland/towerofmadness`. All file:line citations verified in a prior read pass; items marked UNVERIFIED are resolved by a specific test stage below.

---

## 1. Verdict

**Achievable.** Every seam hammurabi's server runtime needs exists in bevy-explorer, and every gap is scoped and small:

**Proven by code reading (no test needed):**
- Full SDK7 scenes run headless with no render/window/GPU (PLAN §2, table row 1).
- Inbound messagebus is already N-room-ready, routed by the scene_id inside the RFC4 Scene packet (crates/comms/src/global_crdt.rs:903-920); sender framing matches the SDK's `decodeCommsMessage` exactly (crates/dcl/src/js/comms.rs:88-94), with client senders arriving as lowercase `0x…` (global_crdt.rs:600-607) matching the scene's `context.from` comparisons (towerofmadness/src/server/server.ts:312).
- Pre-minted LiveKit adapters connect directly (`AdapterManager::connect`, crates/comms/src/lib.rs:287-312); signedFetch, Storage-URL derivation via getRealm, and main.crdt loading are all headless-capable (crates/dcl/src/js/fetch.rs:40-103; initialize_scene.rs:230-232, 468-511).
- The scene's server branch needs **no** physics, raycasts, rendering, audio, or engine-side sync fidelity — entity sync is entirely SDK-side over opaque `sendBinary` bytes (@dcl/sdk network/message-bus-sync.js:95-137; towerofmadness has zero raycasts in src/, triggers computed in scene JS).

**Gaps, all identified with exact fixes (§2b):** `isServer` is hardcoded false (crates/dcl/src/js/modules/EngineApi.js:21-25); outbound routing force-targets the auth server (i.e. itself) for authoritative scenes (crates/restricted_actions/src/lib.rs:1425-1427) plus the cross-room broadcast bug (:1429-1435); `AvatarBase`/`AvatarEquippedData` only propagate via the excluded AvatarPlugin (crates/avatar/src/lib.rs:167-211) — without which `getPlayer()`/`onEnterScene` are silently dead; the room-ready flag (`RealmInfo.isConnectedSceneRoom`) must be true or the SDK server queues every `room.send` forever (crates/scene_runner/src/lib.rs:795-823; message-bus-sync.js RealmInfo.onChange gate).

**What the towerofmadness test will prove (currently UNVERIFIED):** the SDK's `isServer()` Atom resolves true before `main()` reads it; a real client and the bevy server land in the same gatekeeper room (room keying = (realmName, sceneId) is an external-service assumption); the minted server token carries LiveKit identity `authoritative-server` (every client-side routing rule depends on it, livekit/room/plugin.rs:345-347, global_crdt.rs:752); custom CRDT components in main.crdt survive `process_message_stream` (initialize_scene.rs:480-487); ethers v6 + scene `fetch`/signedFetch permission gating in the deno isolate; no ghost server player visible to clients.

---

## 2. Engine changes checklist

### (a) The new bin — `src/bin/headless.rs`

Implement exactly per **PLAN §4** (plugin set, manual inits, fake player + OutOfWorld pinning, guest wallet + `CurrentUserProfile{profile: Some, is_deployed: true}`, PortableScenes pinning, SIGTERM/reaping, CLI, cache-path rules). This document only adds the **server-mode** additions the bin must contain (~500 LOC total):

1. **CLI additions**: `--server-mode` (sets the `IsServer` resource, §2b.1), `--preview` (local realm flow), `--private-key <hex>` (Wallet::finalize instead of guest — wallet/lib.rs:56-84), `--adapter <connstr>` per `--scene` (way-3, pre-minted).
2. **Preview scene discovery** (stage A/B): GET `{realm}/about` → POST `{realm}/content/entities/active` with pointers from `configurations.localSceneParcels` → pin the returned `b64-…` entity id via `PortableScenes::insert(id, PortableSource{ pid: "urn:decentraland:entity:<id>?=&baseUrl={realm}/content/contents/", super_user: false, .. })` (mirrors hammurabi load.ts:136-157; bevy precedent `IpfsIo::active_entities`, crates/ipfs/src/lib.rs:1036-1050; pin API initialize_scene.rs:763). World flow (stage C): parse `about.configurations.scenesUrn`, pin as-is (urns already carry baseUrl; hash extraction initialize_scene.rs:1054-1082).
3. **Server room connect** (stage B/C, bin-local, no engine change): signed POST to comms-gatekeeper `get-server-scene-adapter` with hammurabi's metadata shape `{intent:"dcl:explorer:comms-handshake", signer:"dcl:explorer", isGuest, realm:{serverName}, realmName, sceneId}` sent via `x-identity-metadata` header, empty body — `wallet::sign_request` produces the identical ADR payload (crates/wallet/src/lib.rs:241-259 vs hammurabi signed-fetch.ts:8-24); bevy precedent for header-metadata signed POST at comms/src/lib.rs:231-252. Endpoints: `https://comms-gatekeeper-local.decentraland.org/get-server-scene-adapter` for `b64-` ids (client-side constant precedent comms/src/lib.rs:53-54), `https://comms-gatekeeper.decentraland.org/get-server-scene-adapter` for worlds (hammurabi connect-adapter.ts:50-82). Then `AdapterManager::connect(&adapter)` + tag the transport entity `SceneRoom(scene_hash)` (comms/src/lib.rs:196, 287-312).
4. **Room-ready wiring**: after connect, set `SceneRoomConnection` (crates/comms/src/lib.rs:199) so `RealmInfo.is_connected_scene_room` turns true for the pinned scene (consumed at scene_runner/src/lib.rs:795-823). Single-scene: set the resource from the bin — zero engine change. N-scene: needs the per-scene mapping (§2b.8, deferrable).
5. **AvatarBase replication system** (§2b.7) registered in the bin.
6. **Preview hot-reload for free**: insert `PreviewMode{server: Some(realm), is_preview: true}` — `connect_preview_server` (comms/src/preview.rs:24-72) websockets to the realm and maps SCENE_UPDATE → scene reload (scene_runner/src/util.rs:280-290).
7. **Supervisor**: PLAN M1's check_done pattern + grep-able liveness; note the scene's own logs arrive prefixed `[<display.title> <ts>]` for portables (renderer_context.rs:352-373).

Build note: features `livekit` ON from stage B (PLAN §4 — rooms are the product), which **requires the audio gate** (§2b.5) or pulls kira/cpal init onto the server. Stage A can run `--no-default-features` per PLAN M0.

### (b) Server-mode runtime changes (engine code)

| # | Change | Files / lines | LOC | Needed by |
|---|---|---|---|---|
| 1 | **isServer=true to scene JS.** Add `pub is_server: bool` (`#[serde(default)]`) to `CrdtContext` + ctor (crates/dcl/src/interface/crdt_context.rs:12-26, :37-55). Rides to the sidecar inside `NewSceneInfo.scene_context` (crates/dcl_deno_ipc/src/lib.rs:31) — **no IPC/signature changes**; it's `state.put()` into OpState at crates/dcl/src/js/mod.rs:180. Update 4 call sites: initialize_scene.rs:684-690 (real value from a new `IsServer(bool)` resource, default false — same pattern as `testing`/`preview` there), initialize_scene.rs:472-478 (main.crdt parse ctx, false), comms/src/global_crdt.rs:67, dcl/src/js/mod.rs:156-162. New shared **SYNCHRONOUS** op `op_is_server` returning `bool` — reading `state.borrow::<CrdtContext>().is_server` is a synchronous OpState read, so implement it as `#[op2(fast)] -> bool` (pattern: op_crdt_send_to_renderer) or plain `#[op2] -> bool`. Do NOT model it on the async `op_realm_information` (dcl/src/js/runtime.rs:102 is `#[op2(async)]`, dcl_deno/src/js/op_wrappers/runtime.rs:19,36-39): an async op returns a Promise, so `!!(op && op())` evaluates to `!!Promise === true` unconditionally, and because EngineApi.js is baked into the sidecar and shared by every web/native client (dcl_deno/js/mod.rs:460-462, resolves for `~system/EngineApi` everywhere), that would make EVERY client's `isServer()` Atom resolve true — every normal scene using @dcl/sdk/network would run its server branch on clients. Stage A only runs the server, so its negative control would NOT catch this; it surfaces as clients-behaving-as-servers in Stage B/production. Register deno wrapper (crates/dcl_deno/src/js/op_wrappers/engine.rs:6-8) + wasm mirror (crates/dcl_wasm/src/inner/op_wrappers/engine.rs). Change crates/dcl/src/js/modules/EngineApi.js:21-25 to `return { isServer: !!(Deno.core.ops.op_is_server && Deno.core.ops.op_is_server()) }` — with a **synchronous** op the un-awaited call is a real bool; the feature-detect guard keeps web safe if the wasm wrapper lags. (If it were ever implemented async instead, the JS body MUST `await`: `!!(… && await Deno.core.ops.op_is_server())` inside the already-`async` wrapper — but prefer the sync op.) EngineApi.js is include_str-baked into the **sidecar** (crates/dcl_deno/src/js/mod.rs:460-461) → dcl_deno_ipc rebuild required. Do NOT reuse `is_super` — that maps to `SuperUserScene` privilege (dcl/src/js/mod.rs:197-199). | crdt_context.rs, initialize_scene.rs, global_crdt.rs, dcl/js/mod.rs, runtime-op files, EngineApi.js | ~50 | Stage A |
| 2 | **Outbound recipient fix.** In `send_scene_messages`, do NOT force `NetworkMessageRecipient::AuthServer` when this process IS the auth server (crates/restricted_actions/src/lib.rs:1425-1427) — otherwise every reply publishes to `destination_identities=["authoritative-server"]` = itself (livekit/room/plugin.rs:340-348). Gate on the scene's `is_server`/IsServer resource; preserve `Peer(addr)` (SDK `{to:[context.from]}` → timeSyncResponse targeting) and `All` broadcasts. | restricted_actions/lib.rs | ~10 | Stage B |
| 3 | **Cross-room broadcast fix** (PLAN M3 item 1): skip transports unless `scene_room.0 == ctx.hash` (restricted_actions/lib.rs:1429-1435). Harmless single-room; correctness for N. | restricted_actions/lib.rs | ~5 | Stage B (do together with #2) |
| 4 | **Suppress self-position broadcast** (PLAN M3 item 3): `BroadcastPositionPlugin` added unconditionally (comms/src/lib.rs:43); hammurabi's `reportPosition` is a stub. Gate behind a resource. | comms/lib.rs | ~10 | Stage B |
| 5 | **Audio gate** (PLAN M3 item 2): with `livekit` on, CommsPlugin adds LivekitPlugin → kira AudioManager + per-frame cpal (comms/src/lib.rs:79-80; livekit/plugin.rs:39,135-145); gate mic/kira behind a resource, `audio_track_is_now_subscribed` takes `Option<ResMut<…>>` (track/plugin.rs:423,451). | comms/livekit | ~30 | Stage B |
| 6 | **Suppress realm-wide auto comms connect**: `process_realm_change` joins the about.comms realm room as soon as wallet+realm exist (comms/src/lib.rs:150-181) — a ghost participant in the preview ws-room / world room; hammurabi never joins realm comms. Gate behind a resource. | comms/lib.rs | ~10 | Stage B |
| 7 | **AvatarBase/AvatarEquippedData without AvatarPlugin**: replicate `update_avatar_info` (crates/avatar/src/lib.rs:167-211; registered at :113) as a bin-registered system — `Query<(Option<&ForeignPlayer>, &UserProfile), Changed<UserProfile>>` → `GlobalCrdtState::update_crdt` of AVATAR_BASE + AVATAR_EQUIPPED_DATA; inline `default_bodyshape_urn()` (crates/collectibles/src/base_wearables.rs:294-300) to avoid the collectibles dep. Without it, SDK `onEnterScene`/`getPlayer` never fire (@dcl/sdk/players/index.js:12 requires PlayerIdentityData AND AvatarBase) — `[Server] Player joined` (server.ts:190) and podium wearables (podiumAvatarsServer.ts:225-241) depend on it. UserProfile feed already rides CommsPlugin (profile/mod.rs:303-356,406-519). | src/bin/headless.rs (or shared plugin) | ~50 | Stage B |
| 8 | **Per-scene room connection state**: `SceneRoomConnection` is one global `Option` resource (comms/src/lib.rs:199) → `RealmInfo.is_connected_scene_room` (scene_runner/src/lib.rs:795-823). Single scene: bin sets it (§2a.4, no engine change). N scenes: map SceneRoom(hash) → per-scene flag. | comms + scene_runner | ~40 | Deferrable to N-scene milestone; NOT needed for towerofmadness single-scene test |
| 9 | **`DiscardPlayerUpdates(false)`** in server mode (bin resource value, not a code change): `true` drops ALL player traffic including inbound Scene messagebus packets (global_crdt.rs:421-424) — server would never hear playerJoin/timeSync. | bin only | 1 | Stage B |
| 10 | **Sidecar Child handle**: expose the `Child` from `init_runtime` so the bin can SIGKILL an orphaned sidecar ≤5 s after AppExit (PLAN §4 SIGTERM; plain Command, no PDEATHSIG, dcl_deno_ipc/src/lib.rs:119-124). The one small library change PLAN already flags. | dcl_deno_ipc/lib.rs | ~10 | Stage A |
| 11 | *(Optional)* Suppress `AnnounceProfileVersion` on server room connect (livekit/plugin.rs:74-83) — avoids clients fetching the fake profile. | comms/livekit/plugin.rs | ~5 | Deferrable; observe in stage B ghost check |
| 12 | *(Optional)* `get_connected_players` always appends the local wallet (restricted_actions/lib.rs:1175) → server visible as a connected peer; towerofmadness tolerates it (unregistered players skipped via `gameState.getPlayer()==undefined`). Gate in server mode if the ghost check fails. | restricted_actions | ~5 | Deferrable |
| 13 | *(Optional)* Foreign-player bevy Transform copier (replaces avatar foreign_dynamics.rs:53+) — only for `getPlayersInScene`/ContainingScene RPC correctness (restricted_actions/lib.rs:1181-1209; comms leaves bevy Transforms at origin, global_crdt.rs:459, 537-539). towerofmadness's server path never calls it. | bin | ~30 | Deferrable |

### (c) Headless strip list (waste audit)

Ruthless rule for the first towerofmadness test: **strip only what is free via plugin omission**. Everything needing fork or loader surgery goes to the later table.

| Item | Mechanism | LOC | Deferrable for first test? |
|---|---|---|---|
| Audio/video download+decode | Omit AVPlayerPlugin — PbAudioSource/Stream/VideoPlayer CRDT ids registered only in av crate (av/src/lib.rs:234-241, audio_source.rs:40-43); unregistered deltas drop at scene_runner/src/lib.rs:1014-1017. towerofmadness's 12.9 MB of mp3s never fetched. | 0 | **No — free, already PLAN §4** |
| Standalone scene images (8.4 MB) | Accidental: no ImageLoader in the §4 set → PbMaterial texture loads fail-before-download, materials finalize gracefully (material.rs:355-375, 701-756). | 0 | **No — free.** Caveat: never add ImagePlugin/a stub loader later without re-auditing; fail-before-download is MEDIUM confidence — confirm in stage A (watch for image GETs in the preview server log) |
| Scene UI / TextShape neutralization | Free via OutOfWorld pin + `super_user:false`: `ContainingScene::get` is empty → `layout_scene_ui` skips all scenes, no UiLink, no UI image requests (scene_runner/src/lib.rs:594-605; scene_ui/mod.rs:999-1003) | 0 | **No — free** |
| Animation | KEEP bevy AnimationPlugin for the first test (animated colliders are scene-observable in general; towerofmadness server path verified unaffected). Later `--no-animation` flag must add `init_asset::<AnimationGraph>()` besides AnimationClip (gltf_container.rs:408,427 params panic otherwise) | ~15 | **Yes — later flag** |
| Billboard per-frame tree dirtying | `set_if_neq` on billboard.rs:94,141 (or resource-gate the system) | ~5 | Yes (trivial — take it opportunistically, but not gating) |
| GLTF embedded-texture decode skip | Fork change: restore upstream gate on the texture loop (robtfm/bevy release-0.16-dcl, bevy_gltf/src/loader/mod.rs:516-531, rev fc90189) + headless resource setting `s.load_materials = RenderAssetUsages::empty()` at gltf_container.rs:278. Settings-driven, never a cargo feature | ~30 + fork PR | **Yes — later optimization** |
| NoUi gate (skip UI CRDT registration entirely) | Mirror NoGltf (update_world/mod.rs:153,166-171) around SceneUiPlugin+TextShapePlugin (mod.rs:177-178). DuiPlugin must stay (restricted_actions/lib.rs:1484) | ~15 | Yes |
| VideoTexture retry churn fix | Make `Tex::VideoTexture` terminal instead of `SourceNotReady`→`RetryMaterial` every-frame loop (material.rs:399-418,481-485) | ~10 | Yes (towerofmadness has none) |
| Double transform propagation | scene_runner re-runs bevy propagation (transform_and_parent.rs:69-83, admitted hack) | — | Yes — measure in PLAN M4 first |

**Later-optimization expected savings** (measured on towerofmadness's 32.9 MB assets/): texture-decode skip converts the 9.1 MB of embedded GLB texture payload (78% of GLB bytes) from decoded-and-permanently-resident (no render extraction to free RENDER_WORLD assets — structural inference, UNVERIFIED at runtime; `reduce_image_sizes` caps at ~4 MiB/texture, fork Cargo.toml:119, loader/mod.rs:1030,1146) to zero decode/residency; combined with the free omissions, ~65% of content bytes are never fetched and only ~2.6 MB of mesh/animation payload does real work.

---

## 3. Test protocol with towerofmadness

Scene facts that shape all stages: server entry `main()` → `if (isServer()) { server(); return }` (src/index.ts:380-392); `authoritativeMultiplayer: true`, world `boedo.dcl.eth`, 45 parcels base 0,0 (scene.json:15-115). **Before any stage: rebuild the scene** — the repo's main.crdt (13 Jul) is newer than src/ (4 Jul); schema drift causes the "Outside of the bounds of written data" crash on both runtimes.

**Preview port note (applies to every command that references `localhost:8000`):** the preview port is **not** fixed at 8000. sdk-commands computes it as `getPort(options.args['--port'] || 0)` (start/index.js:187), and `getPort(0)` resolves via `portfinder.getPortPromise({port:0})` (logic/get-free-port.js:8-16), which starts at basePort 8000 but returns the next FREE port if 8000 is occupied (e.g. a leftover preview/hammurabi process). If it lands on 8001+, a hardcoded `--realm http://localhost:8000` connects the headless server to nothing. Either pass an explicit `--port 8000` to `npm start` (flag exists, start/index.js:73) or read the exact origin `start` prints ('Available on:' / bevy URL, index.js:255-291) and pass that to `--realm`/`--server`. The commands below pass `--port 8000` for determinism.

### Stage A — LOCAL: headless server branch boots against the preview realm

**Prerequisites**: engine changes §2b #1 and #10; bin per §2a items 1-2, 6-7. No comms needed — the server branch boots, initializes state, and hits Storage before any room activity (room.send merely queues until room-ready).

**Commands**:
```bash
# 1. Scene: rebuild + serve preview
cd /Users/boedo/Documents/Decentraland/towerofmadness
npm run build                     # sdk-commands build → bin/index.js + fresh main.crdt
npm start -- --port 8000          # preview realm pinned to http://localhost:8000
# npm start ALSO unconditionally spawns `npx @dcl/hammurabi-server@next --realm=…`
# (spawnAuthServer, start/index.js:239-241) — this call is UNCONDITIONAL, before any
# skipClient checks. There is NO start flag that suppresses it: --no-client / skipClient
# (index.js:131) only gates the explorer/browser launch (index.js:292,307), not the
# auth-server child. So for the headless-only run you MUST kill it after npm start:
pkill -f hammurabi-server

# 2. Engine (two invocations, sidecar first — PLAN §4)
cd /Users/boedo/Documents/Decentraland/bevy-explorer
cargo build --release --package dcl_deno_ipc --locked
cargo build --release --bin headless --no-default-features --locked

# 3. Run headless as server
./target/release/headless --realm http://localhost:8000 --preview --server-mode --timeout 120
```

**Success criteria** — grep the headless stdout (scene logs arrive prefixed `[<display.title> <ts>]`, renderer_context.rs:352-373). All strings verbatim from scene source, in boot order:

| String | Source | Proves |
|---|---|---|
| `holaa com  sssas` | index.ts:63 (module scope) | JS bundle executed |
| `[Game] Starting...` | index.ts:381 | main() ran |
| `[TimeSync] Initializing server-side handler` | timeSync.ts:132 | isServer Atom true at init |
| `[Game] Running as SERVER` | index.ts:389 | **the isServer gate — the whole point of stage A** |
| `[Server] Tower of Madness starting...` | server.ts:18 | server() entered |
| `[Server] Initializing game state...` / `[Server] Game state initialized` | gameState.ts:214/330 | syncEntity setup survived |
| `[Server] New round: round_` + `[Server] Tower created:` + `[Server] TriggerEnd positioned at y=` | gameState.ts:564/538/524 | game loop ticking, entity pool built |
| `[Server] Ready` | server.ts:161 | full boot |
| `[Server][Storage] Loaded global leaderboard:` (or the caught `Failed to load` error) | gameState.ts:919-923 | signedFetch + getRealm + preview storage routes (`/values`, `/env` served by sdk-commands storage-service.js:21-56) |

**Negative control**: `[Game] Running as CLIENT` (index.ts:394) must NOT appear.

**Failure diagnostics**: `Running as CLIENT` → isServer op returned false or resolved too late (SDK caches it in an Atom at module load, @dcl/sdk/network/index.js:8-13 — the CrdtContext plumbing sets it before scene main(), so this indicates a wiring bug); no storage lines → getRealm/`RealmInfo` not seeded (needs real `starting_realm`, initialize_scene.rs:437-466) or signedFetch bailed "no player identity" (CurrentUserProfile missing, dcl/src/js/mod.rs:228-239); "Outside of the bounds of written data" → stale main.crdt, rebuild; 3 redundant `/about` fetches at startup are expected noise (retry predicate ignores `localSceneParcels`, ipfs/src/lib.rs:907-921). This stage also resolves UNVERIFIED: main.crdt custom-component survival through process_message_stream, and whether scene fetch/signedFetch hit PermissionManager denials headless. **Note:** because Stage A runs only the server, its negative control does NOT catch the async-op mistake in §2b.1 (an async `op_is_server` makes `!!Promise` true — server still boots correctly; the bug only shows up as clients-acting-as-servers in Stage B). Confirm the op is synchronous by code review before Stage B.

**Hammurabi comparison run** (same preview):
```bash
cd /Users/boedo/Documents/Decentraland/hammurabi-headless
npm run dev        # build + node dist/cli.js, default realm http://localhost:8000 (cli.ts:85-87)
```
Capture both stdouts; diff the ordered sequence of the table's strings plus storage results. They must be identical modulo timestamps/prefixes.

**Effort**: ~2-3 days (isServer plumbing + bin skeleton + this run).

### Stage B — COMMS LOCAL: headless server + real client in one room

**Prerequisites**: stage A green; engine changes §2b #2-#7, #9; bin items §2a.3-4; rebuild with `--features livekit` (audio gate #5 mandatory now). Requires reachability of `comms-gatekeeper-local.decentraland.org` (UNVERIFIED: its permissiveness for arbitrary b64 sceneIds and guest signers).

**Commands**:
```bash
# scene preview still running (stage A, pinned --port 8000), hammurabi child killed
cd /Users/boedo/Documents/Decentraland/bevy-explorer
cargo build --release --package dcl_deno_ipc --locked
cargo build --release --bin headless --features livekit --no-default-features --locked   # feature set per PLAN M0 fallback rules
./target/release/headless --realm http://localhost:8000 --preview --server-mode
# bin: signed POST get-server-scene-adapter (LocalPreview metadata: realmName "LocalPreview",
# sceneId <b64 urn>) → AdapterManager::connect → SceneRoom(hash) + SceneRoomConnection set

# real client, option 1 (native bevy):
./target/release/decentra-bevy --preview --server http://localhost:8000
# option 2 (web): PASTE THIS URL MANUALLY into a browser — a plain `npm start` runs
# explorer-alpha and auto-opens sortedURLs[0], NOT the bevy-web URL (the bevy-web URL is
# only built/printed/opened on the web-explorer path, index.js:280,307-310). So it will
# NOT auto-open under the default invocation; the format itself is correct:
#   https://decentraland.zone/bevy-web/?preview=true&realm=http://localhost:8000&position=0,0
```
The client, standing in the scene, emits `SetCurrentScene{realm_name:"LocalPreview", scene_id:"b64-…"}` → local gatekeeper `get-scene-adapter` → same LiveKit room (scene_runner/src/lib.rs:1113-1153; comms/src/lib.rs:226-230).

**Success criteria**:
1. **Time-sync round trip (best single liveness proof)**: server log `[TimeSync] Server received sync request from 0x…, responding with t2=` (timeSync.ts:140) ~5x within a second of client join; client log `[TimeSync] Synchronized, offset:` (timeSync.ts:223) and `[Game] Connected to server` (index.ts:206). Proves inbound messagebus, room-ready flag, AND targeted outbound (`{to:[context.from]}`, timeSync.ts:142-151 → Peer recipient → destination_identities, livekit/room/plugin.rs:340-348).
2. **Player presence**: server log `[Server] Player joined: <name>` (server.ts:190) — proves the §2b.7 AvatarBase replica (onEnterScene requires PlayerIdentityData+AvatarBase, @dcl/sdk/players/index.js:12) with a real display name, not the `address.substring(0,8)` fallback (server.ts:164-171).
3. **State sync to client — which entities**: client sees the server-authored tower — the chunk pool (Transform/GltfContainer/Visibility, gameState.ts:284-320), TriggerEnd platform (Transform/MeshRenderer/Material, gameState.ts:213-283), 3 podium AvatarShapes + 3 TextShape leaderboards (podiumAvatarsServer.ts:104-131) — delivered via SDK CRDT sync (REQ/RES_CRDT_STATE on join, then deltas; message-bus-sync.js:95-137). Visual check in the client: tower visible, round timer counting (timeSync-driven), chunks rearrange on `[Server] New round:`.
4. **No ghost server**: client sees exactly 1 avatar (itself); server identity not rendered, no "guest" joining chat. Resolves UNVERIFIED #11/#12 and confirms position-broadcast suppression (#4).
5. **Gameplay round-trip**: walk to the tower start → server logs `[Server] Player started attempt:` (server.ts:238); climb → `[Server] Finish attempt: height=` (server.ts:280).
6. 30 min soak: round cycle logs every ~7 min (`[Server] Round ended!` gameState.ts:638 → `[Server] ENDING phase done after` → `[Server] BREAK phase done after`, server.ts:72,80); no `broken` scene, flat RSS.

**Failure diagnostics** (map directly to the known failure modes):
- **Client runs the server branch** (`[Game] Running as SERVER` on the CLIENT, normal scenes misbehaving) → `op_is_server` was implemented async and the un-awaited Promise is truthy (§2b.1); make the op synchronous. This is the failure Stage A cannot surface.
- `[Server] Ready` logged but NO timeSync responses ever → **room-ready deadlock**: `is_connected_scene_room` never true (SceneRoomConnection not set / RealmInfo not refreshed, scene_runner/src/lib.rs:795-823) — the SDK queues all room.send forever (message-bus-sync.js RealmInfo.onChange gate). REQ/RES_CRDT_STATE still works in this state (bypasses the queue), so "client sees tower but timer frozen" is the exact signature.
- timeSync requests never RECEIVED → DiscardPlayerUpdates still true (global_crdt.rs:421-424), or client and server are in different rooms (room-keying UNVERIFIED — inspect both adapter URLs' room names), or the server token identity is not `authoritative-server` (client targets that identity, restricted_actions/lib.rs:1425-1427; decode the LiveKit JWT from the adapter URL to check — resolves the biggest UNVERIFIED).
- Responses sent but client never syncs → recipient override not fixed (#2: server addressed itself) — grep server-side livekit publish destinations.
- `[Server] Player joined:` missing while timeSync works → AvatarBase replica (#7) broken or UserProfile fetch failing (profile/mod.rs:303-356).
- Podium debug: up to 1500-char `room.send('podiumDebug')` payloads at round end (podiumAvatarsServer.ts:159-178) stress outbound chunking exactly then — watch for drops at round boundaries.

**Effort**: ~4-5 days (bulk of the engine changes) + 1 day of runs.

### Stage C — WORLD: boedo.dcl.eth end-to-end

**Prerequisites**: stage B green. Scene deployed to worlds-content-server (`npm run deploy` in towerofmadness targets .org/.zone, package.json:12-13). Note the naming wrinkle: scene.json worldConfiguration says `boedo.dcl.eth` while hammurabi's dev:world and multiplayerId say `towerofmadness` — pin the realm explicitly to whichever world the deploy actually targeted.

**Commands**:
```bash
./target/release/headless --realm boedo.dcl.eth --server-mode          # guest identity
# or with an orchestrator key:
./target/release/headless --realm boedo.dcl.eth --server-mode --private-key <hex>
# bin: map_realm_name → https://worlds-content-server.decentraland.org/world/boedo.dcl.eth
# (ipfs/src/lib.rs:672-678); pin first scenesUrn entry; signed POST to prod
# get-server-scene-adapter with realmName "boedo.dcl.eth".
# Client: any explorer at realm boedo.dcl.eth (bevy: --server boedo.dcl.eth).
```

**Success criteria**: same six checks as stage B, plus: hammurabi parity run `cd hammurabi-headless && npm run dev:world` (targets `towerofmadness.dcl.eth` — adjust to the deployed world) produces the same observable sequence.

**Failure diagnostics / UNVERIFIED resolved here**: prod `get-server-scene-adapter` rejecting guest signers (hammurabi's dev:world implies guests work today; production multiplayer-server signs with `AUTHORITATIVE_SERVER_PRIVATE_KEY`, `isGuest:false` — if rejected, supply `--private-key`); actual shape of the world's `/about` scenesUrn; **storage auth**: prod `storage.decentraland.org` expects hammurabi's delegation-signed requests (`x-authoritative-scope` + ephemeral key, connect-context-rpc.ts:53-112) which bevy lacks — expect `Failed to load` storage lines and leaderboards resetting each boot; ACCEPTED for this smoke test, tracked in §5. EnvVar `prizeWalletKey` empty → MANA prizes disabled → ethers-v6-in-deno stays UNVERIFIED (only exercised at tournament end; acceptable).

**Effort**: ~1-2 days.

---

## 4. Comparison gate vs hammurabi

All must pass (stage B for 1-5, C for 6, both for 7) before calling bevy-headless a replacement:

| Check | Bevy evidence | Hammurabi reference behavior |
|---|---|---|
| isServer branch | `[Game] Running as SERVER`, never `…as CLIENT` | modules.ts:29-31 answers `{isServer:true}` before main() |
| Player presence (join/leave/name) | `[Server] Player joined: <real name>`; leave after client quits (10 s staleness despawn, global_crdt.rs:923-940) | avatar feed via scene-context.ts:668-672 |
| Messagebus IN | `[TimeSync] Server received sync request from 0x…` | CommsTransportWrapper sceneId routing |
| Messagebus OUT (targeted + broadcast) | client `[TimeSync] Synchronized`; broadcasts (`tournamentStarted`, `teleportToBase`) observed client-side | room.send `{to:[from]}` + broadcast via LiveKit identities |
| State sync | client renders tower/TriggerEnd/podium/leaderboards; chunks rearrange on new round | RES_CRDT_STATE + deltas, identical (SDK-side) |
| No ghost server | client sees no extra avatar/peer; scene player loops unpolluted | server is invisible (reportPosition stub, connect-adapter.ts:98-100) |
| Memory snapshot | RSS/PSS of engine+sidecar at 1 scene ≤ 1 hammurabi worker after 30 min (full benchmark = PLAN M4, not this gate) | one `node dist/cli.js` process |

---

## 5. Out of scope / later

- **Orchestrator seam**: multiplayer-server integration — `PROCESS_SCENE_ID/REALM_URL/POSITION/COMMS_ADAPTER/ENVIRONMENT` env mirror (single-scene drop-in) and the multi-scene spawn seam (repeated `--scene/--adapter` pairs breaks the 1-process-per-scene env contract — needs a multiplayer-server change). Way-3 mapping is already specified in the exploration; per PLAN §1 explicitly out of scope.
- **Benchmark / go-no-go**: PLAN M4 verbatim (PSS on Linux, tiers 1-4, thresholds table). The towerofmadness stages are correctness gates, not the memory verdict.
- **Storage delegation for production worlds**: replicate hammurabi's `x-authoritative-scope` + delegated ephemeral key signing (connect-context-rpc.ts:53-112, minted via PROCESS_STORAGE_DELEGATION) or an allow-listed orchestrator key via `Wallet::finalize`. Blocks persistent leaderboards in prod only.
- **Texture-strip optimizations** and the rest of §2c's deferrable rows (fork gltf gate, NoUi, `--no-animation`, VideoTexture churn fix, double-propagation) — take after the comparison gate passes, savings quantified in §2c.
- **Per-scene kill/limits + ForeignPlayer slot reuse** (~401-player lifetime cap per engine, global_crdt.rs:440-490) — PLAN risks 4/7; matters for long-lived rounds with churn, measured in the M4 soak.
- **Per-scene SceneRoomConnection mapping** (§2b.8) — required only when one engine hosts N server scenes.