# Browser headless startup regression

Use a current web build in `deploy/web/engine/pkg` and the local Vite page.
Serve the existing raw SDK7 fixture (no scene compilation needed):

```sh
python3 local-harness/scene-kill/serve.py 8111
```

Open the web explorer at `http://localhost:5173/`, wait for its engine to load,
and run this in the `app.html` frame's developer console:

```js
await window.__bevyStartServer({
  ...window.__bevyBootConfig,
  realm: 'http://localhost:8111',
  position: '0,0',
  preview: true
})
```

Before the fix, the server worker panics on its first update in
`restricted_actions::external_url`: `NonSend<'_, ()>` is missing. No scene runs.
With the fix, the worker keeps running; the `graceful` fixture logs
`KILLTEST|graceful|scene-start` and recurring `KILLTEST|graceful|alive` lines.
Read them from the `headless.html` frame with `window.sceneLogsAfter(0)`.
Remove that frame when finished to stop the test server.

The client and server own separate Bevy worlds. Both must register the marker
used by `JsThread` systems before their first update.
