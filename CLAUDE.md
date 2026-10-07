# bevy-explorer

- `react-web/`: the web explorer's HUD (React). Read `react-web/AGENTS.md` before changing it.
- `deploy/web/`: what the web build ships: the page, the engine's JS, the service worker.
- `crates/`, `src/`: the engine (Rust). It runs natively and, built to wasm, in the browser. A
  local wasm build follows CI's "Build WASM package" step in `.github/workflows/ci.yml`.
- The scene editor hosted by the web explorer lives in `dcl-regenesislabs/bevy-editor`; its
  `docs/WEB-EDITOR.md` covers developing both repos together.
