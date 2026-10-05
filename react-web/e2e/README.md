# e2e — real-engine validation (tier 2)

Per-domain tests that drive the **live** app — the React HUD, the bevy engine (in a
same-origin iframe), and the super-user bridge scene — and assert each API call
round-trips over the bridge. The player is driven with **bevy console commands**
(`walk_player_to`, `teleport`, `player_position`); panels are opened with real DOM
clicks; the bridge is observed via a `BroadcastChannel` spy.

This complements the deterministic **tier 1** suite (`src/test/*.test.tsx`, run with
`npm test`), which covers *every* API call per domain — including the ones a fresh
guest can't reach (accepting a friend request, leaving a community, marking
notifications read). Tier 2 proves the guest-reachable calls actually work end to end.

## Requirements

- **A real GPU.** The engine uses WebGPU + `SharedArrayBuffer`; it cannot run headless.
  Tests run headed (`headless: false`).
- A local engine build in `../deploy/web` (the `pkg/` wasm).
- Chromium for Playwright: `npx playwright install chromium`.

## Run

```bash
# from react-web/
npx playwright install chromium      # once
npm run test:e2e
```

`playwright.config.ts` starts both servers automatically (Vite dev on :5173 + the
bridge scene on :8100, with `BRIDGE_SCENE_PREVIEW=0` so Vite doesn't also start its
own) and reuses them if already running. To point at an already-running app, set
`E2E_URL=http://localhost:5173`.

List the tests without launching the engine:

```bash
npm run test:e2e -- --list
```

## What it covers

One test per domain, in `engine.spec.ts` (boots the world once, serial):

| Domain | Driven by | Asserts (bridge) |
|---|---|---|
| session | enter as guest | `getProfile`, `getNotifications` sent on entry |
| world (move) | `walk_player_to` | `player_position` changes |
| chat | type + Enter | `sendChat` |
| settings | click Settings | `getSettings` → `settings` |
| emotes | click Emotes | `getEmotes` → `emotes` |
| wearables + avatarPreview | click Backpack | `getWearables` → `wearables`, `engineViewport` |
| communities | click Communities | `getCommunities` → `communities` |
| gallery | click Gallery | `getGallery` → `gallery` |
| world (map) | click Map | `getMap` → `mapState` |
| profile | relay + click Profile | `profile`, panel active |
| notifications | click Notifications | `getNotifications` → `notifications` |
| friends | click Friends | `friends` snapshot |
| world (mic) | click Voice chat | `setMic` → `mic` |
| world (teleport) | `teleport` | `player_position` changes |
| pointer, nametags | — | world-space / data-dependent → covered in tier 1 |

Data-dependent **actions** (friend accept/reject/cancel/block, community join/leave,
notification mark-read, wearable equip/preview, emote play, setting change, gallery
photo metadata/delete) are asserted in tier 1, where the state can be injected
deterministically.

## Sibling tier 1.5 — visual regression (no engine)

`visual.spec.ts` (separate config: `../playwright.visual.config.ts`) screenshots **every DOM
domain** in mock mode (`?mock=1`) — headless, no GPU, deterministic — and diffs against committed
baselines in `visual.spec.ts-snapshots/`. This is the fast "did the UI change?" gate; the engine
suite here is the slow "does it round-trip?" gate. `playwright.config.ts` ignores `visual.spec.ts`,
so `npm run test:e2e` only runs the engine tests.

```bash
npm run test:visual            # check
npm run test:visual:update     # refresh baselines (then eyeball the PNGs before committing)
```

## Preview-realm gate (real engine, own config)

`preview-realm.gate.spec.ts` builds the starter scene **in the page** with the scene editor's
`@dcl-editor/web-build`, publishes it to the service worker's preview realm
(`../../deploy/web/PREVIEW_REALM.md`), boots the engine into that realm, then edits the source,
rebuilds, and sends `/reload <entityId>`. It passes when the engine reads the entity again and the
scene logs the new version marker. No server holds the scene; the worker's own request log is the
evidence, printed at the end of the run.

```bash
# a dcl-editor checkout, after `npm run build -w @dcl-editor/web-build`
WEB_EDITOR_DIR=/path/to/dcl-editor npx playwright test --config playwright.gate.config.ts preview-realm
```

It starts its own Vite server on `GATE_PORT` (`e2e/vite.gate.config.ts`: the app's config plus the
web-build files under `/web-build/`) and a bridge scene of its own on `GATE_BRIDGE_PORT` (never
reused, so a dev server's :8100 is left alone); every spec loads the page with `?bridgePort=`
pointing at it (`e2e/gate.ts`).
`npm run test:e2e` skips it.

## Editor gates (real engine, same config)

Five specs drive the scene editor package inside this page with real clicks. They need a built
engine in `deploy/web/engine/pkg` (`just wasm`), the TS bindings
(`just ts-bindings`), and a dcl-editor checkout whose package speaks host contract v1.1 or later (`mountHome`):

```bash
# in the dcl-editor checkout; any --editor-base will do: the page reads only the entity id from
# `scene/about` and fetches the scene from its own /editor/scene/, checked against that id
npm run export-static -w @dcl-editor/scene
npm run build -w @dcl-editor/web
# here: every gate, or one by name (preview-realm, editor)
WEB_EDITOR_DIR=/path/to/dcl-editor npx playwright test --config playwright.gate.config.ts [name]
```

`editor.gate.spec.ts` also runs against a production build: `GATE_ENTRY=/bevy-web` loads the
page at that no-slash entry with the bundled bridge scene (no `?bridgePort`), and
`GATE_EDITOR_BASE=<url>/` is where the pinned package is served from (README "Releasing the
editor"). The servers are then yours to start.

Besides Vite and the bridge scene, the config starts two servers out of that checkout (Node 24,
no build): its project storage service, over a directory that starts empty
(`.vite/gate-projects-<service port>`). The page is
pointed at the service with `?editor-projects=` (`e2e/gate.ts`).

### Running gates side by side

Every port a gate run takes comes from `e2e/ports.ts`; all four must be free, and none is ever
reused:

| Variable | Default | Serves |
|---|---|---|
| `GATE_PORT` | 5230 | the page (Vite) |
| `GATE_BRIDGE_PORT` | 8110 | the bridge scene (`?bridgePort=`) |
| `GATE_SERVICE_PORT` | 8787 | the project storage service (`?editor-projects=`) |

Two runs at once each take their own set. In the same checkout they also keep their own Vite
cache, project data and `test-results/gate-<page port>/`, but build the same `bridge-scene/bin`,
so start the second once the first's bridge scene is up, or use another checkout:

```bash
WEB_EDITOR_DIR=/path/to/dcl-editor npx playwright test --config playwright.gate.config.ts e2e/editor.gate
WEB_EDITOR_DIR=/path/to/dcl-editor GATE_PORT=5330 GATE_BRIDGE_PORT=8210 GATE_SERVICE_PORT=8887 \
  npx playwright test --config playwright.gate.config.ts e2e/editor.gate
```

A name filter is matched against the whole path, so in a checkout whose path contains `editor`,
`editor` alone runs every gate; `e2e/editor.gate` does not.

| Spec | What passes |
|---|---|
| `editor.gate.spec.ts` | As a guest: nothing of the editor loads before the sidebar's Create button is clicked; the Create page opens with the editor's list in its body; New scene previews the starter from the service worker under a random id and the engine travels there; the editor's scene spawns privileged next to the HUD's without logging in over the player; a code save reloads only that scene, by id, and keys typed in the editor never reach the canvas; Play and Stop restart only the project scene, never the HUD's; Back to scenes and Back to Decentraland give the realm, clock and HUD back; the menu's Create item reopens without refetching the editor; `?editor=<project>` opens the scene directly. Nothing is asked of the project service. |

The player starts in a one-scene realm the gate's Vite server answers at `/gate-home`; the spec
aborts (and fails on) any deployment to a host that is not local, and drops the engine's analytics.

`GATE_SHOTS=<dir>` keeps a screenshot of each screen.

See **`../review.md`** for the full harness overview, per-domain expectations, the world-space
agent checklist, and the pre-merge review checklist.
