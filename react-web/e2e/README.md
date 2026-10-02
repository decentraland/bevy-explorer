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

It starts its own Vite server on :5230 (`e2e/vite.gate.config.ts`: the app's config plus the
web-build files under `/web-build/`) and the bridge scene on :8100 (reused if already running).
`npm run test:e2e` skips it.

## Editor gate (real engine, same config)

`editor.gate.spec.ts` opens the app with `?editor` as a guest and drives the scene editor package
with real clicks: a scene from the Example starter (stored, built and published in the browser),
the editor's own scene attached to it, an entity and a code edit that rebuild and reload it by id,
Play, Stop, and Exit back to the HUD. Its six steps pass or fail one by one; the run prints them.

```bash
# in the dcl-editor checkout: the editor scene's `about` holds an absolute url, so export it for the gate's port
npm run export-static -w @dcl-editor/scene -- --editor-base http://localhost:5230/editor/
npm run build -w @dcl-editor/web
# here
WEB_EDITOR_DIR=/path/to/dcl-editor npx playwright test --config playwright.gate.config.ts editor
```

`GATE_SHOTS=<dir>` keeps a screenshot of each screen. The player starts in a one-scene realm the
gate's Vite server answers at `/gate-home` (nothing is fetched from a catalyst). Give the config a
name (`editor` or `preview-realm`): with none it runs both gates.

See **`../review.md`** for the full harness overview, per-domain expectations, the world-space
agent checklist, and the pre-merge review checklist.
