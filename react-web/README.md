# react-web — React DOM port of the bevy-ui HUD

A React DOM reimplementation of the SDK7 react-ecs HUD, living in the
**bevy-explorer** repo. It renders the chrome as DOM overlays on the explorer's
own page and drives the engine over a thin **super-user bridge scene** instead of
drawing UI inside the engine.

The UI being ported lives in the separate **`bevy-ui-scene`** repo
(`scene/src/ui-classes`, `scene/src/bevy-api`); the `scene/...` paths below refer
to that repo.

> **This app IS production.** CI builds it (`vite build`) into `deploy/web/` — the tree published
> as `@dcl-regenesislabs/bevy-explorer-web` and served at the explorer URL. The React page owns the
> root `index.html`; the engine boots IN the same document — no iframe, no old boot page — from
> `deploy/web/engine/` (boot module + workers + wasm); the bridge scene ships at
> `deploy/web/bridge-scene/static`. Only the
> *sources* stay here at the repo root — build artifacts in `deploy/web` are git-ignored.
> See **Deploy (production)** below.

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

One command — vite starts the bridge scene's live preview (:8100, scene hot-reload) alongside
the app unless one is already running:

```bash
npm install && (cd bridge-scene && npm install)   # once
npm run dev
```

**Engine mode (default)** — `http://localhost:5173/`: real engine in the SAME document
(canvas at z-0 behind the HUD; engine module from `../deploy/web/engine`), with
`systemScene=http://localhost:8100` (the bridge scene). React login → **Explore as Guest** (`/login_guest`) → scene-loading overlay
(real data) → world. Needs a local engine build at `../deploy/web`.

**Mock mode** — `http://localhost:5173/?mock=1`: full UI (login + scene-loading) on
a fake bridge, no engine. Add `&previousLogin=1` for the returning-user flow.

## Deploy (production)

Everything ships in the one `@dcl-regenesislabs/bevy-explorer-web` package (the `deploy/web`
tree), published by CI's **Build and Deploy Web** job on merge to `main` and served at the
explorer URL (e.g. `decentraland.zone/bevy-web`, assets on the versioned CDN path). Layout:

| Path in `deploy/web` | What | Built by |
|---|---|---|
| `index.html` + `assets/` … | **this React app** (the production page) | `vite build` (CI) |
| `engine/` | engine boot module + workers + `pkg/` (wasm) — no page | `wasm-pack` (CI) |
| `bridge-scene/static/` | the exported bridge-scene realm | `npm run bundle` (CI) |
| `service_worker.js` | shared root-scope SW: rewrites COEP → `credentialless` | tracked |

**URL rules (learned the hard way):**
- The page is served at a **no-trailing-slash entry** (`/bevy-web`) while assets live on the
  **versioned CDN** — so the React build uses an *absolute* base (`PUBLIC_URL`, from
  `deploy/web/scripts/prebuild.js` → `package.json.homepage`) and never `./`-relative refs
  in `index.html`.
- The **engine module + bridge scene + service worker must stay same-origin** with the page
  (BroadcastChannel / `contentWindow`): they resolve against `PAGE_DIR`
  (`src/lib/publicUrl.ts`), *never* against the CDN base.

```bash
# CI does, in order (see .github/workflows/ci.yml build-deploy-web):
wasm-pack build --out-dir deploy/web/engine/pkg …   # engine wasm
npm i                 # in deploy/web — prebuild.js stamps PUBLIC_URL/homepage
PUBLIC_URL=<homepage> npm run build                 # in react-web — the HUD → deploy/web
npm run bundle        # in react-web/bridge-scene — realm → deploy/web/bridge-scene/static
# then oddish publishes deploy/web → npm + CDN
```

Local prod-shape check: build the three pieces, then `npx serve deploy/web` (serve.json carries
the COOP/COEP headers) and open `http://localhost:3000`.

**Test the bundled scene in dev** — append `?bundled=1` to the app URL. Instead of the live
preview realm (`sdk-commands start` on :8100), the engine loads the exported static bundle vite
serves from `/bridge-scene/static` — i.e. exactly what ships in prod. (No `?bundled` → live
preview, fast iteration with scene hot-reload.)

## Testing

Two tiers cover every domain's bridge API and the clicks that drive them:

- **Tier 1 — deterministic (`npm test`, vitest + Testing Library).** A `FakeDriver`
  records every page→scene API call and injects scene→page responses, so each domain
  test drives the real `useEngineSession` hook and asserts: every action posts the exact
  wire message, and every inbound message updates state. Covers all 13 domains —
  *including* calls a guest can't reach (accept request, leave community, mark read).
  Plus **click** tests that render each real component and assert every button's
  expected result (login CTAs, sidebar nav, chat send + emoji/members, friend
  accept/reject/cancel/unblock, settings toggle/slider/select/reset, backpack
  preview/equip, community join/leave/add-friend/open-chat, map jump-in/teleport,
  notifications mark-read, menu nav, profile-chip sign-out/exit, emote play, …).
  Files in `src/test/*.clicks.test.tsx`. Runs in CI (no engine).
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
- [x] **Integration (Approach A)** — the engine runs in the React document itself (no iframe,
  old boot page deleted); `deploy/web/engine/boot.js` is the whole boot surface.
