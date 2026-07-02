# Follow-ups (post production-launch)

Queued work after #901 (React HUD as the production page). Ordered by value.

## 1. Integration "Approach A" — no iframe
Mount React in the engine's own document and drop the iframe + old boot page.
- Move the load-bearing bits of `deploy/web/engine/{index.html,ui.js,main.js}` into React or a
  small loader module: boot-progress globals (`__bevyLoadProgress/Step`), panic capture
  (`__bevyPanic`, `bevy-crash`), crash watchdog/heartbeat, `__bevyLaunch`, SW registration.
- `deploy/web/engine/` shrinks to ≈ wasm `pkg/` + the workers (same-origin files by spec).
- Kills the iframe-boundary complexity at the root: double SW registration dance, postMessage
  crash bridging, `contentWindow` plumbing, per-page COEP concerns.
- DELETE the old page outright — it is already non-functional standalone: its in-engine HUD came
  from the old bevy-ui-scene systemScene, but the default systemScene is now the HEADLESS bridge
  (renders nothing, relays to React), so nothing draws UI or closes the loader without React.
  Engine debugging happens through the React page (`?position=`, `?realm=`, console commands).

## 2. Host-level fixes at the zone edge (infra, not this repo)
- Redirect `/bevy-web` → `/bevy-web/` — removes the first-visit SW reload bounce at the source
  (the no-slash entry is outside the SW scope; see `src/lib/coiServiceWorker.ts`).
- Serve `COEP: credentialless` for the app path — removes the SW dependence for the page's
  catalyst `<img>` loads entirely.
- Fix `marketing-files.decentraland.org` CORS: it hardcodes `ACAO: decentraland.org`, breaking
  events banners for zone/localhost origins (fails through the SW's cacheFirstStrategy).

## 3. Defer the emoji chunk
~533 KB raw / 46 KB gz of emoji data is modulepreloaded on the initial page. Load it behind the
chat emoji picker/autocomplete instead — see the note in `src/features/chat/emojiData.ts` and the
`manualChunks` comment in `vite.config.ts`.

## 4. CI-wire the visual suite (tier 1.5)
Baselines are darwin-only and the config trusts a leftover server (`reuseExistingServer: true`).
For CI: generate Linux baselines, set `reuseExistingServer: !process.env.CI`.

## 5. Small cleanups
- `engine/favicon/` duplicates `react-web/public/favicon/` (~100 KB; engine page still refs its
  own copy — dedup when Approach A lands).
- `e2e/visual.spec.ts` `enterWorld` and `e2e/helpers.ts` `enterAsGuest` duplicate the
  login→skip-picker flow — share one helper.
