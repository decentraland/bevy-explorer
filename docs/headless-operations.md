# bevy-headless: operations handbook

The one-stop reference for running, releasing and supporting the headless engine —
both of its consumers:

- **SDK preview**: `sdk-commands start` spawns the npm launcher as the local
  authoritative server (see [headless-sdk-preview.md](./headless-sdk-preview.md)).
- **Orchestrated server**: the multiplayer-server
  ([decentraland/sdk-multiplayer-server](https://github.com/decentraland/sdk-multiplayer-server),
  `SCENE_WORKER_BACKEND=bevy-engine`) runs ONE long-lived engine hosting every
  authoritative scene, spawned reactively from room events.

Roadmap: tracked in
[sdk-multiplayer-server#131](https://github.com/decentraland/sdk-multiplayer-server/issues/131)
(phases: [#126](https://github.com/decentraland/sdk-multiplayer-server/issues/126)
engine migration & hardening,
[#127](https://github.com/decentraland/sdk-multiplayer-server/issues/127) creator
observability, [#128](https://github.com/decentraland/sdk-multiplayer-server/issues/128)
tooling & ecosystem,
[#129](https://github.com/decentraland/sdk-multiplayer-server/issues/129) scale &
stress testing,
[#130](https://github.com/decentraland/sdk-multiplayer-server/issues/130) default-on
decision).

## Risks

What can go wrong and what contains it:

- **Untrusted scene code, shared engine.** All co-tenant scenes run in one process.
  Containment layers: per-scene sandbox caps (CRDT ingress, per-tick RpcCalls,
  WebSocket resource/send caps, deno ops-table lockdown), per-room presence
  isolation ([PRESENCE_ISOLATION.md](./PRESENCE_ISOLATION.md)), and orchestrator-side
  crash attribution + quarantine (two attributed fast crashes → 10-min cooldown).
  **Residual risk:** a scene that goes live and later crashes the engine natively
  (segfault/OOM) takes co-tenants down with it until respawn; attribution from
  outside can't catch it. Engine-side isolation work is the fix (roadmap phase 1).
- **Key isolation.** The authoritative private key never enters the engine — it
  boots with a throwaway guest wallet and asserts on anything else
  (`src/bin/headless.rs`). Real authority is minted per scene by the orchestrator:
  room-scoped comms adapters, scene-scoped world-storage delegations (short TTL,
  base-parcel-pinned). A full engine compromise is confined to its scenes' storage
  for at most the TTL.
- **Shared signing identity.** Non-storage `signedFetch` from server scenes is
  signed with the engine's guest wallet — one address shared by all co-tenant
  scenes per engine incarnation. Services must key on the signed metadata
  (`sceneId`/`parcel`), never on the signer address alone.
- **Single point of failure per task.** Engine death drops every scene on the task
  until the bounded respawn re-adds them (staggered, attributable). Engine-level
  outages alert to Slack (`:rotating_light:`, rate-limited) with stable
  `ALERT bevy-engine-*` log markers.
- **Platform.** Only `linux-x64` is published for servers — the orchestrator image
  must build `linux/amd64`. macOS/Windows packages exist for dev machines and the
  SDK preview.

## How to test

- **Engine-only / SDK preview**: [headless-sdk-preview.md](./headless-sdk-preview.md)
  — build commands, `npm link` flow, `DCL_SERVER_PACKAGE` for the full SDK spawn
  path, and a validated end-to-end checklist.
- **Against the orchestrator**: multiplayer-server's
  [`docs/bevy-engine-packaging-and-testing.md`](https://github.com/decentraland/sdk-multiplayer-server/blob/main/docs/bevy-engine-packaging-and-testing.md)
  — `BEVY_ENGINE_BIN` at a local build for the fast loop, `DCL_BEVY_SERVER_PATH`
  to also exercise the npm resolver, plus the staged `.zone` checklist.
- **Isolation/lifecycle harnesses**: `local-harness/presence/` (per-room presence
  isolation, forged cross-room bus) and `local-harness/scene-kill/` in this repo.
- **CI**: every publish builds all four platforms and smoke-boots the binary
  (engine + sidecar start and shut down cleanly) before anything publishes —
  a red build never ships.

## How to release

`.github/workflows/publish-headless.yml`, one trigger = one meaning:

- **Push to main** (touching engine code) → snapshot published on the **`next`**
  dist-tag as `0.1.0-<runId>.commit-<sha>`. Automatic, not consumed by default.
- **Manual dispatch from `main`** → **release**: builds the ref and publishes to
  **`latest`**, which SDK previews track. There is no build-only dispatch and no
  tag choice — clicking "Run workflow" ships.

After a release, bump the orchestrator: pin the exact version in
multiplayer-server's `package.json` (`npm view @dcl-regenesislabs/bevy-headless-server dist-tags`
to find it), sync both lockfiles, PR, then walk the `.zone` checklist. Only the
launcher package's dist-tag matters — it pins the platform packages by exact
version.

## Reporting bugs

Report in the **#multiplayer-server** Slack channel. Include:

- The **world/scene** (`sceneId` or world name) and a timestamp.
- The **engine version** — from the orchestrator's startup log
  (`Engine binary: …`) or the pinned `@dcl-regenesislabs/bevy-headless-server`
  version (`0.1.0-<runId>.commit-<sha>` — the sha is the engine commit).
- **Scene logs**: the `/logs` SSE stream (creators) or the `/stats-ui` log panel
  (operators, non-prd) around the timestamp.

Engine bugs graduate to issues in this repo; orchestrator bugs to
[sdk-multiplayer-server](https://github.com/decentraland/sdk-multiplayer-server/issues).

## How to roll back

Fastest first — every step is config, no code change:

1. **Whole backend off**: set `SCENE_WORKER_BACKEND` back to `hammurabi` (or unset
   it) on the deployment. Scenes fall back to the one-fork-per-scene node backend;
   no image change needed.
2. **Engine version only**: repin the previous
   `@dcl-regenesislabs/bevy-headless-server` version in multiplayer-server and
   redeploy — versions are immutable on npm, so any previous pin is a valid target.
   (`BEVY_ENGINE_BIN` can also force an arbitrary binary in an emergency.)
3. **SDK preview**: `DCL_SERVER_ENGINE=hammurabi` switches a developer back; the
   SDK also falls back automatically when the launcher exits 78 (unsupported
   platform).
4. Last resort: redeploy the previous image tag.
