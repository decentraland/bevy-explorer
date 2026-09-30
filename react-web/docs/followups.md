# Follow-ups (post production-launch)

Queued work after #901 (React HUD as the production page). Ordered by value.

## 1. Host-level fixes at the zone edge (infra, not this repo)
- Redirect `/bevy-web` → `/bevy-web/` — removes the first-visit SW reload bounce at the source
  (the no-slash entry is outside the SW scope; see `src/lib/coiServiceWorker.ts`).
- Serve `COEP: credentialless` for the app path — removes the SW dependence for the page's
  catalyst `<img>` loads entirely.
- Fix `marketing-files.decentraland.org` CORS: it hardcodes `ACAO: decentraland.org`, breaking
  events banners for zone/localhost origins (fails through the SW's cacheFirstStrategy).

## 2. Defer the emoji chunk
~533 KB raw / 46 KB gz of emoji data is modulepreloaded on the initial page. Load it behind the
chat emoji picker/autocomplete instead — see the note in `src/features/chat/emojiData.ts` and the
`manualChunks` comment in `vite.config.ts`.

## 3. CI-wire the visual suite (tier 1.5)
Baselines are darwin-only and the config trusts a leftover server (`reuseExistingServer: true`).
For CI: generate Linux baselines, set `reuseExistingServer: !process.env.CI`.

## 4. Small cleanups
- `e2e/visual.spec.ts` `enterWorld` and `e2e/helpers.ts` `enterAsGuest` duplicate the
  login→skip-picker flow — share one helper.

## 5. De-flake tier-2 against live services
A single flaky test (catalyst deploy latency, live social data) aborts the whole serial file and
cascades ("13 did not run"). Add per-test retries for the live-service assertions, or split the
world-entry cost from the domain walks so one flake doesn't sink the run.
