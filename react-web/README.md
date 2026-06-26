# react-web — React DOM port of the bevy-ui HUD

A React DOM reimplementation of the SDK7 react-ecs HUD, living in the
**bevy-explorer** repo. It renders the chrome as DOM overlays on the explorer's
own page and drives the engine over a thin **super-user bridge scene** instead of
drawing UI inside the engine.

The UI being ported lives in the separate **`bevy-ui-scene`** repo
(`scene/src/ui-classes`, `scene/src/bevy-api`); the `scene/...` paths below refer
to that repo.

> **Production is untouched.** This app sits at the repo root, NOT under
> `deploy/web`, so it is excluded from the `@dcl-regenesislabs/bevy-explorer-web`
> npm publish (which ships the whole `deploy/web` tree) and from the cargo/wasm
> build. Never move it under `deploy/web` — it would bloat the package.

## Why

The SDK7 react-ecs UI (~50k LOC) is hard to debug and animate, and can't share a
real design system. DOM React gives us animations, devtools, CSS, and a token
system. See `docs/REACT-UI-FOR-BEVY-EXPLORER.md` in the `dcl-editor` repo for the
embedding background.

## Architecture

```
React DOM page  ──BroadcastChannel('bevy-ui-bridge')──►  super-user bridge scene  ──►  SystemApi
   (this app)   ◄──────── events / rpc responses ───────   (slim SDK7 scene)
```

- `SystemApi` and `BroadcastChannel` are exposed to the **super-user `--ui`
  scene only**, so React can't call the engine directly — a bridge scene relays.
- The page-side client (`src/engine/bridge.ts`) is **transport-agnostic**: it only
  touches a `BroadcastChannel`, so it works whether the engine is in this document
  (Milestone 2a) or a same-origin iframe (Milestone 2b). Only where the bridge
  scene lives changes, not this code.

### Files

| Path | Role |
|---|---|
| `src/engine/protocol.ts` | Wire types for the page↔scene protocol (mirrors `scene/src/bevy-api/interface.ts`). |
| `src/engine/bridge.ts` | Page-side `BridgeClient` — RPC correlation + events over `BroadcastChannel`. |
| `src/engine/mockBridge.ts` | Dev-only fake "scene" that answers the protocol so the UI runs with **no engine**. |
| `src/features/login/` | First slice: loading + login (ports `ui-classes/loading-and-login`). |
| `src/styles/tokens.css` | Design tokens ported from `scene/src/utils/constants.ts`. |

## Run

Two processes — the bridge scene (super-user SDK7 scene that relays engine streams)
and the React dev server:

```bash
# 1. the bridge scene (live preview realm — MUST be sdk-commands start, not export)
cd bridge-scene && npm install && npx sdk-commands start --no-browser --port 8100

# 2. the React app + engine
npm install && npm run dev
```

**Engine mode (default)** — `http://localhost:5188/`: real engine in a same-origin
iframe (`../deploy/web`), with `systemScene=http://localhost:8100` (the bridge
scene). React login → **Explore as Guest** (`/login_guest`) → scene-loading overlay
(real data) → world. Needs a local engine build at `../deploy/web`.

**Mock mode** — `http://localhost:5188/?mock=1`: full UI (login + scene-loading) on
a fake bridge, no engine. Add `&previousLogin=1` for the returning-user flow.

## Testing

Two tiers cover every domain's bridge API and the clicks that drive them:

- **Tier 1 — deterministic (`npm test`, vitest + Testing Library).** A `FakeDriver`
  records every page→scene API call and injects scene→page responses, so each domain
  test drives the real `useEngineSession` hook and asserts: every action posts the exact
  wire message, and every inbound message updates state. Covers all 13 domains —
  *including* calls a guest can't reach (accept request, leave community, mark read).
  Plus component **click** tests (sidebar nav, chat send, friend actions, …). Files in
  `src/test/`. Runs in CI (no engine).
- **Tier 2 — real engine (`npm run test:e2e`, Playwright).** Boots the live app + bridge
  scene, enters as a guest, drives the player with **bevy console commands**
  (`move_player_to`, `teleport`) and real clicks, and asserts each API call round-trips
  over a BroadcastChannel spy. Needs a real GPU (WebGPU, headed) — see `e2e/README.md`.

```bash
npm test            # tier 1 (fast, deterministic)
npm run test:e2e    # tier 2 (real engine; local, needs a GPU)
```

## Status

- [x] **Login slice** (loading / sign-in-or-guest / secure-step / reuse) — guest +
  previous via `/login_*` console commands. End-to-end to the world.
- [x] **Scene-asset loading in React** — `SceneLoadingOverlay` driven by the bridge
  scene's `getSceneLoadingUIStream` relay; shows on initial entry AND every teleport.
- [x] **Bridge over BroadcastChannel** — bridge scene served via **`sdk-commands
  start`** (NOT `export-static`; the static realm's `comms:offline` stops the relay).
  Renders no UI → suppresses the old HUD.
- [ ] **`loginNew` (new-account code flow)** — relay is in the bridge scene; wire the
  page's `startLoginNew` through the bridge instead of the console driver.
- [ ] **Use the real `bevy-ui-scene`** as the bridge: move the relay into it, trim its
  react-ecs flat UI, keep only 3D/world-space (nametags, pointer events). Hybrid:
  SDK7 keeps 3D + bridges; React renders all flat UI.
- [ ] Port remaining slices (chat, menu/settings, profile, map, friends, …).
- [ ] **Integration (Approach A)** — mount React in the explorer's own
  `deploy/web/index.html` (no iframe); transport-agnostic client unchanged.
