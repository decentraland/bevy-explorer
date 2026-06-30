# bevy-explorer — project instructions

(General coding style lives in `~/.claude/CLAUDE.md`. This file adds project-specific context.)

## What this repo is right now

We are **rebuilding the in-world UI / HUD in React** under **`react-web/`**, replacing the old
approach of an in-engine **system-scene / scene-ui**. New UI work happens in `react-web/`, **not**
in a Decentraland SDK7 scene.

- **`react-web/`** — the active React DOM HUD. It hosts the prebuilt engine (Rust/WASM) in a
  same-origin iframe (served at `/engine/` from `deploy/web`) and drives it over a bridge scene.
  This is where UI is built now.
- **`deploy/web/`** — the prebuilt engine bundle (WASM + its own loader). Treat as vendored build
  output; only touch its `ui.js` / `main.js` / `index.html` for host-integration shims (e.g.
  surfacing boot progress to react-web). The `pkg/*.wasm` is git-ignored, CI-built.

## Related repositories (siblings under `~/dev/protocol-squad/`)

Reference these for behaviour/feature parity or the shared wire protocol — do **not** build new HUD
UI in them.

- **`bevy-ui-scene`** (`../bevy-ui-scene/`) — the previous in-engine **system-scene / scene-ui** we
  are migrating **away** from. Reference for behaviour / feature parity only; do not build new UI there.
- **`social-service-ea`** (`../social-service-ea/`) — the Social Service backend (TypeScript). Proto
  definitions and API endpoints for friendships, blocking, etc.
- **`social-rpc-client-js`** (`../social-rpc-client-js/`) — JS RPC client for the social service.
- **`js-sdk-toolchain-ps`** (`../js-sdk-toolchain-ps/`) — fork/branch of the DCL SDK toolchain used
  for protocol-squad development.
- **`protocol-ps`** (`../protocol-ps/`) — fork of `decentraland/protocol` (canonical `@dcl/protocol`
  proto definitions), currently on the `protocol-squad` branch. **Source of truth for the wire
  protocol** shared between scenes (via `js-sdk-toolchain-ps`) and the runtime (via `bevy-explorer`).
  Edit a `.proto` here when you need a new SDK component / message / RPC; both downstreams consume the
  regenerated types.

## Design-system rules (authoritative for all react-web UI)

Follow the guidelines in @../react-web/AGENTS.md — token-driven only (no hardcoded hex / radii /
type sizes), use or **create** `src/design/` primitives, honor `--ui-scale` on floating UI, and
verify floating/portaled UI visually in `?mock=1`. Read it before building or changing HUD UI.

## Working agreements

- `cd react-web && npm run typecheck` (or `npx tsc --noEmit`) and `npm test` must pass before
  claiming done.
- jsdom / vitest can't see layout — **verify visual & responsive changes in the browser** (preview
  MCP; `?mock=1` for the full HUD without the engine). Don't claim a layout/responsive fix works on
  tests alone.
- **External links must open securely.** Any external `target="_blank"` anchor or
  `window.open(url, '_blank', …)` MUST set `rel="noopener"` / `'noopener'` to block reverse-tabnabbing
  (the opened page can't reach `window.opener`). Don't add `noreferrer` — sending the `Referer` is fine.

## Local dev / preview

- The user runs their **own** vite dev server on port **5173**. The preview launch config
  (`.claude/launch.json`) uses a **separate** port (5199) — vite ignores the `PORT` env var, so the
  port is passed via `--port`. Don't try to take 5173.

## Git: branches, commits & PRs  ← IMPORTANT

**Branch model.** `main` ← **`feat/react-web-hud`** (the integration branch — *everything* lands here,
then goes to `main` later). Work branches stack on top of it and are **numbered**: `fix/react-web-hud`
is **PR #0** (current); subsequent ones are `fix/01-<something>`, `fix/02-<something>`, … each based on
the previous branch (stacked / sequential — see below).

- **Commit messages: NEVER add a `Co-Authored-By: Claude …` trailer** (overrides the global default).
  No Claude attribution in commits on this project.
- **PRs: the USER decides when to open one.** I prepare the branch and may **suggest** opening a PR,
  but I run `gh pr create …` **only after the user explicitly confirms** — never open a PR on my own.
  Likewise, only commit/push when the user asks.
- **Stacked PRs (sequential).** Each branch is based on the **previous** branch, not `main`:
  `fix/01-*` on `fix/react-web-hud`, `fix/02-*` on `fix/01-*`, … so work continues without waiting for
  the lower PR to merge. The bottom of the stack targets `feat/react-web-hud`. Create each PR with
  `gh pr create --base <previous-branch>` (diff then shows only its own changes). Merge **bottom-up**;
  **restack** the upper branches after any lower PR is updated or merged (`git rebase --onto …`, then
  `git push --force-with-lease`). Squash-merge footgun: after a lower PR is squash-merged, replay only
  the upper commits with `git rebase --onto <new-base> <old-base-branch> <upper-branch>`.

## Local engine wasm (`deploy/web/pkg`)

> **NEVER compile the wasm automatically — ALWAYS ask first.** The build takes ~15–20 min, so do
> not run `wasm-pack build` (or any engine wasm rebuild) without explicit user confirmation, even
> when the local wasm is clearly stale. Diagnose, recommend the rebuild, then wait for the user's go.

The engine WASM is **CI-built and git-ignored**, so the local copy can lag the Rust source in
`crates/`. Symptom: the engine replies `Command not recognized: /…` for a console command that DOES
exist in the source (e.g. `/login_identity`) → the local wasm is stale. Rebuild it from the local
Rust, **from the repo root**:

```sh
wasm-pack build --target web --out-dir ./deploy/web/pkg --no-default-features --features="livekit,social"
echo "{\"wasmSize\":$(wc -c < ./deploy/web/pkg/webgpu_build_bg.wasm)}" > ./deploy/web/pkg/manifest.json
```

`--no-default-features` is **required**: the default features (`ffmpeg`, `inspect`) pull native deps
that can't cross-compile to wasm (build fails on `ffmpeg-sys-next` / pkg-config). After it finishes
(~15–20 min), full-reload the browser so the iframe serves the new wasm, and keep `manifest.json` in
sync so the download-progress % stays accurate.

## Self-correction protocol  ← IMPORTANT

When I (the assistant) get something wrong, or the user corrects me:
1. Stop and restate the **correct** approach as a general, reusable instruction.
2. Append it to the **Corrections log** below as a dated bullet, phrased so the mistake does not
   recur ("Do X, not Y — because Z"). Keep it short and imperative.

## Corrections log

<!-- Newest first. Format: - YYYY-MM-DD — <rule>. Why: <reason>. -->
- 2026-06-29 — To verify boot/download progress UI (the engine `DOWNLOADING… %` / footer bar),
  don't rely on the live preview: localhost caches the WASM so the download is instant and you
  can't catch intermediate %. Verify with a deterministic render test (mock `flow` with
  `engineReady: false`, a `loadProgress`, `loadStep: 'download'`) instead. Why: avoids "I couldn't
  reproduce it" loops on fast-loading assets.
