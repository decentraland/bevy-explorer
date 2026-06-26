# e2e — real-engine validation (tier 2)

Per-domain tests that drive the **live** app — the React HUD, the bevy engine (in a
same-origin iframe), and the super-user bridge scene — and assert each API call
round-trips over the bridge. The player is driven with **bevy console commands**
(`move_player_to`, `teleport`, `player_position`); panels are opened with real DOM
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
bridge scene on :8100) and reuses them if already running. To point at an
already-running app, set `E2E_URL=http://localhost:5173`.

List the tests without launching the engine:

```bash
npm run test:e2e -- --list
```

## What it covers

One test per domain, in `engine.spec.ts` (boots the world once, serial):

| Domain | Driven by | Asserts (bridge) |
|---|---|---|
| session | enter as guest | `getProfile`, `getNotifications` sent on entry |
| world (move) | `move_player_to` | `player_position` changes |
| chat | type + Enter | `sendChat` |
| settings | click Settings | `getSettings` → `settings` |
| emotes | click Emotes | `getEmotes` → `emotes` |
| wearables + avatarPreview | click Backpack | `getWearables` → `wearables`, `engineViewport` |
| communities | click Communities | `getCommunities` → `communities` |
| world (map) | click Map | `getMap` → `mapState` |
| profile | relay + click Profile | `profile`, panel active |
| notifications | click Notifications | `getNotifications` → `notifications` |
| friends | click Friends | `friends` snapshot |
| world (mic) | click Voice chat | `setMic` → `mic` |
| world (teleport) | `teleport` | `player_position` changes |
| pointer, nametags | — | world-space / data-dependent → covered in tier 1 |

Data-dependent **actions** (friend accept/reject/cancel/block, community join/leave,
notification mark-read, wearable equip/preview, emote play, setting change) are
asserted in tier 1, where the state can be injected deterministically.

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

See **`../review.md`** for the full harness overview, per-domain expectations, the world-space
agent checklist, and the pre-merge review checklist.
