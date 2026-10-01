# bevy-headless: operations handbook

The headless engine has two consumers: the **SDK preview** (`sdk-commands start`
spawns the npm launcher — see [headless-sdk-preview.md](./headless-sdk-preview.md))
and the **orchestrated server**
([sdk-multiplayer-server](https://github.com/decentraland/sdk-multiplayer-server),
`SCENE_WORKER_BACKEND=bevy-engine`), where one long-lived engine hosts every
authoritative scene.

Roadmap: [sdk-multiplayer-server#131](https://github.com/decentraland/sdk-multiplayer-server/issues/131).

## Risks

- **Per-scene behavior is sandboxed and isolated by design**: CRDT ingress and
  per-tick RpcCalls caps, WebSocket resource/send caps, deno ops lockdown,
  per-room presence isolation ([PRESENCE_ISOLATION.md](./PRESENCE_ISOLATION.md)),
  storage confined by scene-scoped delegations, comms by room-scoped adapters.
  The remaining shared surface is the **process fault domain**: a native fault or
  OOM driven by one scene's content restarts every co-tenant scene. Mitigated by
  the orchestrator's crash attribution + quarantine; closed fully by
  [#126](https://github.com/decentraland/sdk-multiplayer-server/issues/126).
- **Key isolation**: the authoritative key never enters the engine (it asserts on
  booting with anything but a throwaway guest wallet); real authority is minted
  per scene by the orchestrator, so a full engine compromise is bounded to its
  scenes' storage for at most the delegation TTL.
- **Platform**: servers are `linux-x64` only — build the image `linux/amd64`.

## How to test

- Engine-only / SDK preview: [headless-sdk-preview.md](./headless-sdk-preview.md).
- Against the orchestrator: multiplayer-server's
  [`docs/bevy-engine-packaging-and-testing.md`](https://github.com/decentraland/sdk-multiplayer-server/blob/main/docs/bevy-engine-packaging-and-testing.md)
  (`BEVY_ENGINE_BIN` for the fast loop, plus the staged `.zone` checklist).
- Harnesses: `local-harness/presence/`, `local-harness/scene-kill/`.

## How to release

`.github/workflows/publish-headless.yml`: a **push to main** publishes a `next`
snapshot (`0.1.0-<runId>.commit-<sha>`); a **manual dispatch from main** is a
release — it publishes to `latest`, which SDK previews track. Then pin the exact
version in multiplayer-server (`package.json` + both lockfiles) and walk the
`.zone` checklist.

## Reporting bugs

Post in **#multiplayer-server** with: the world/`sceneId` and a timestamp, the
engine version (the pin, or `Engine binary:` in the startup log — the
`commit-<sha>` suffix is the engine commit), and scene logs from the
creator-permissioned `/logs` stream (the scene's creator has to pull them —
there is no operator log access). Engine bugs graduate to issues here;
orchestrator bugs to
[sdk-multiplayer-server](https://github.com/decentraland/sdk-multiplayer-server/issues).

## How to roll back

All config, fastest first:

1. `SCENE_WORKER_BACKEND` back to `hammurabi` (or unset) — whole backend off, no
   image change.
2. Repin the previous engine version in multiplayer-server (npm versions are
   immutable; `BEVY_ENGINE_BIN` can force an arbitrary binary in an emergency).
3. SDK preview: `DCL_SERVER_ENGINE=hammurabi` (automatic on launcher exit 78).
4. Redeploy the previous image tag.
