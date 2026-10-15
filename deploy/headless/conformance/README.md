# Headless scene-conformance test

CI-runnable end-to-end check that the headless engine still executes a real SDK7
scene correctly: `run.py` serves the fixture scene in `scene/` from a minimal
in-process realm (about + active-entities + contents), boots the engine against
it, and asserts the five `[CONFORMANCE]` lines the scene prints — scene boot,
`isServer() == true`, a raycast hit against a collider, tween state progression
and tween completion. Exit code 0 only when all assertions pass and the engine
exits cleanly.

```
python3 deploy/headless/conformance/run.py [path/to/headless]
```

The engine binary defaults to `$CONFORMANCE_ENGINE_BIN`, then
`target/debug/headless`. The `dcl_deno_ipc` sidecar must sit next to the engine
binary (the engine spawns it from its own directory). Python 3 stdlib only.

`scene/bin/index.js` is committed prebuilt so CI never needs an SDK toolchain.
To rebuild it after editing `scene/src/index.ts`: `npm install && npm run build`
inside `scene/`.
