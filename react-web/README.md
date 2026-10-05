# react-web — React DOM port of the bevy-ui HUD

A React DOM reimplementation of the SDK7 react-ecs HUD, living in the
**bevy-explorer** repo. It renders the chrome as DOM overlays on the explorer's
own page and drives the engine over a thin **super-user bridge scene** instead of
drawing UI inside the engine.

The UI being ported lives in the separate **`bevy-ui-scene`** repo
(`scene/src/ui-classes`, `scene/src/bevy-api`); the `scene/...` paths below refer
to that repo.

> **This app IS production.** CI builds it (`vite build`) into `deploy/web/` — the tree published
> as `@dcl-regenesislabs/bevy-explorer-web` and served at the explorer URL. The root `index.html` is
> a small shell page (`src/shell/`) that holds the sign-in key and embeds the React app, `app.html`,
> in an iframe isolated in a process of its own. The engine boots IN the app's document from
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

- `SystemApi` and `BroadcastChannel` are exposed to the **super-user `--system-scene`
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
`systemScene=http://localhost:8100` (the bridge scene). React login → **Explore as Guest** (`/login_guest`) → lobby → **Jump in** → scene-loading overlay
(real data) → world. Needs a local engine build at `../deploy/web`.

The lobby enables **Customize** once the player is ready. Its landing card uses the saved home
destination; native clients retrieve it through the bridge. Failed realm travel from the startup
lobby returns to the lobby so another destination can be chosen.

**Mock mode** — `http://localhost:5173/?mock=1`: full UI (login + scene-loading) on
a fake bridge, no engine. Add `&previousLogin=1` for the returning-user flow.

**Scene editor (dev)** — the editor is an external package (the `dcl-editor` repo's
`packages/web/dist`); this app only hosts it, and only on localhost (or a deployment with a
released package — see **Releasing the editor**). Where it is available the sidebar rail
and the menu top bar get **Create**, a menu page whose body the editor renders its home into
(`src/features/editorHost/CreatePage.tsx`). Nothing of the editor is requested until the page
opens, which it also does once in-world on a url with `?editor` (`?editor=<projectId>` also names
the project to open). In mock and native mode there is no editor.

```bash
WEB_EDITOR_DIR=<dcl-editor checkout, packages/web built> npm run dev
# then click Create in the rail, or open http://localhost:5173/?editor
```

Vite serves `$WEB_EDITOR_DIR/packages/web/dist` same-origin under `/editor/`. The HUD itself only
knows whether to offer Create (`editorHost/config.ts`); the rest is a chunk of its own
(`editorHost/EditorHost.tsx`, a `React.lazy` boundary) that loads when Create first opens, or at
start with `?editor`. It sets `window.__dclEditorHost` (`host/host.ts` — the contract, v1.1) and
loads `<editor base>editor.js`, which leaves `window.__dclEditor`. The page calls its
`mountHome(body, { close })`, and the returned unmount when the page closes. Opening a scene closes
the page and docks the editor into the host's container; its "back to scenes"
(`host.openCreatePage()`) and Exit (`host.exit()`) both leave the scene and travel back, the first
then reopening the Create page. The host talks to the bridge scene on its own channel
(`host/bridge.ts`: its travels and its scene), and its signing code (`host/signer.ts`, with the
curve library) is a separate script added the first time the editor signs.

The package must hold `editor.js`, `web-build/` and `scene/`: the editor scene's `export-static`
output (`scene/about` plus the content files named by hash). The page never runs those files from
the package's host: `spawnEditorScene` fetches the entity, checks it and every file it lists
against their hashes (`host/editorScene.ts`), stores them, and the service worker serves them at
`<PAGE_DIR>editor-scene/<entityId>`, which is the realm the engine spawns. The `baseUrl` inside
`scene/about` is not used, so the export needs no `--editor-base`.

The host signs for the editor with the signed-in wallet's stored identity; the host API never
hands the key over. That is a property of the API, not an isolation boundary: the package runs in
this page's own JavaScript realm, so the pin, its integrity hash and the allowed hosts are what
keep untrusted code out. `signedFetch` signs only urls under the project storage service, with
the query, the host's fixed metadata and only the `accept`, `content-type`, `if-match` and
`if-none-match` headers the editor sets; any method is allowed, so the editor can also delete the
player's stored scenes. Beside a `decentraland.zone` or `.org` Worlds server, `signedFetch` also
signs as a scene's signed fetch would (the path without the query, the editor's metadata) for that
environment's Worlds, comms-gatekeeper, storage and multiplayer `/logs`, and creators-data `/v2`, with
a method allow-list per service; it refuses the project service's own metadata there, and removing a
scene from a world (`DELETE /world/<name>/scenes/<parcel>`) waits for the page's own confirmation.
The `x-confirm-delete-all` header passes too. `signDeployment` signs only after the page's own confirmation dialog, and
only the entity id: the world is inside that entity, but the dialog shows the world and server the
editor reports. `engineConsole` passes only the commands the editor uses to inspect and drive the
scene it edits; `reload` and `set_scene` must name that scene or the editor's own. A guest gets
`not-signed-in`. On localhost the services are
`http://localhost:8787` (projects) and the production Worlds content server; `?editor-projects=<url>`
and `?editor-worlds=<url>` point them at other local (loopback) services, on localhost only. Before signing a deployment the
page's dialog names the world, scene, files, the Worlds server's host and the wallet; it keeps
the editor's fixed size and only Cancel or Sign close it.

When the previewed scene's `scene.json` has `authoritativeMultiplayer: true`, `openPreview` first
starts its scene server beside the client (`__bevyStartServer`, a headless engine in a hidden frame,
`deploy/web/engine/headless.js`); one runs at a time, and leaving the preview or opening another
removes its frame. Its scene logs go to that frame's console; the editor's Storage tab cannot
reach its storage yet.

In dev, `?bridgePort=<port>` loads the bridge scene from `http://localhost:<port>` instead of
:8100 (the gates run their own); production builds ignore it.

### Releasing the editor

A production build has no editor at all (none of its code ships) until a package is pinned. To release one:

1. Publish the editor's `packages/web/dist` to a **versioned, immutable** CDN directory that sends
   `Access-Control-Allow-Origin: *` and `Cross-Origin-Resource-Policy: cross-origin` (the page is
   cross-origin isolated, and `editor.js` loads with `crossorigin`).
2. Set `PINNED_EDITOR` in `src/features/editorHost/config.ts`:
   - `base`: that directory, with the trailing slash;
   - `editorJsIntegrity`: `sha384-` + `openssl dgst -sha384 -binary editor.js | openssl base64 -A`.
     The editor's own manifest of its worker, wasm, snapshot and css (sha384 each) is inside
     `editor.js`, so this one hash covers them; checking those is the editor's job, not the page's;
   - `editorSceneEntity`: the editor scene's entity id, the `urn:decentraland:entity:<id>` in
     `scene/about` (also printed by `export-static`). The page spawns only that entity, from bytes
     it has checked against it; a file that does not match is refused and nothing is spawned;
   - `hosts`: the deployment hostnames that offer Create (loopback always does).

   `PINNED_SERVICES` in `source.ts` says which project service and Worlds server that package talks to.

A pinned production build (`vite build`) loads the released package on loopback too; the dev
server always serves its own `/editor/`.

## Deploy (production)

Everything ships in the one `@dcl-regenesislabs/bevy-explorer-web` package (the `deploy/web`
tree), published by CI's **Build and Deploy Web** job on merge to `main` and served at the
explorer URL (e.g. `decentraland.zone/bevy-web`, assets on the versioned CDN path). Layout:

| Path in `deploy/web` | What | Built by |
|---|---|---|
| `index.html` + `app.html` + `assets/` … | **this React app**: the shell page and the app it embeds | `vite build` (CI) |
| `engine/` | engine boot module + workers + `pkg/` (wasm) — no page | `wasm-pack` (CI) |
| `bridge-scene/static/` | the exported bridge-scene realm | `npm run bundle` (CI) |
| `service_worker.js` | shared root-scope SW: isolates `app.html` (Document-Isolation-Policy), drops COEP from the shell, rewrites COEP → `credentialless` elsewhere | tracked |
| `preview_realm.js` | the SW's preview realm and editor-scene realm for the scene editor (`PREVIEW_REALM.md`), loaded with `importScripts` | tracked |

**URL rules (learned the hard way):**
- The page is served at a **no-trailing-slash entry** (`/bevy-web`) while assets live on the
  **versioned CDN** — so the React build uses an *absolute* base (`PUBLIC_URL`, from
  `deploy/web/scripts/prebuild.js` → `package.json.homepage`) and never `./`-relative refs
  in `index.html` / `app.html`.
- The **engine module + bridge scene + service worker must stay same-origin** with the page
  (BroadcastChannel / `contentWindow`): they resolve against `PAGE_DIR`
  (`src/lib/publicUrl.ts`), *never* against the CDN base.
- The sandbox worker is loaded as **`pkg/sandbox_worker.bundle.js`**. It runs its scene on the
  scene runtime (`crates/dcl_scene_wasm`, built to `pkg-scene/`) in that runtime's own memory,
  and never loads the engine's glue. Scene code shares the worker's realm, and any module in a
  realm can be re-imported by URL, so the runtime's glue is inlined: its exports become
  module-scope bindings of the bundle. A scene can still import the bundle's own URL, but a
  namespace exposes only a module's **exports** and this entry has none, so it gets an empty
  object. **Keep `sandbox_worker.js` export-free** — anything it exports becomes reachable. The
  bundle embeds a copy of the generated glue, so it must be rebuilt whenever the wasm is.

```bash
# CI does, in order (see .github/workflows/ci.yml build-deploy-web):
wasm-pack build --out-dir deploy/web/engine/pkg …   # engine wasm
wasm-pack build crates/dcl_scene_wasm --out-dir ../../deploy/web/engine/pkg-scene …   # scene runtime
npx esbuild engine/sandbox_worker.js --bundle …     # inlines the glue — see note below
npm i                 # in deploy/web — prebuild.js stamps PUBLIC_URL/homepage
PUBLIC_URL=<homepage> npm run build                 # in react-web — the HUD → deploy/web
npm run bundle        # in react-web/bridge-scene — minified production realm → deploy/web/bridge-scene/static
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
- **Scene editor gates (`playwright.gate.config.ts`).** `preview-realm`: a scene built in the page
  by the scene editor's web-build, served from the service worker's preview realm and hot-reloaded
  in the real engine. `editor`: what the page owes the editor package — Create loads nothing until
  clicked, a scene is previewed under a random id, the editor's privileged scene attaches, a code
  save reloads only that scene, Play and Stop never restart the HUD scene, and leaving gives the HUD
  back. The editor's own flows (sync, publish, npm) are tested in its repo. Both need a dcl-editor
  checkout — see `e2e/README.md`.

```bash
npm test            # tier 1 (fast, deterministic)
npm run test:e2e    # tier 2 (real engine; local, needs a GPU)
```

Run the lobby and carousel regression checks without a browser:

```bash
npm test -- src/test/lobbySession.test.tsx src/test/lobby.test.tsx src/test/rail.test.tsx
```

These cover player readiness, persisted native homes, failed travel recovery, carousel navigation,
and the CSS contracts for the avatar cutout. DOM hit-testing still needs browser verification.

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
