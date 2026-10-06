# Preview realm (contract v0)

A Decentraland realm served by `service_worker.js` out of Cache Storage, so a scene built in the
browser loads in the engine with no server. The page (the scene editor) writes the store; the
worker (`preview_realm.js`) only reads it.

## Where

- `<PAGE_DIR>` is the directory the service worker is registered at (its scope).
- A realm lives at `<PAGE_DIR>preview/<projectId>/`, `projectId` matching `/^[a-z0-9][a-z0-9-]{0,63}$/`.
- Every request under `<PAGE_DIR>preview/` is answered by the worker and never reaches
  `ipfs-path-cache-v2`. Anything it does not hold is a 404. One thing is not the project's to
  hold: the pointers that are not parcels (see Routes).

## Store

Cache Storage, cache `dcl-editor-preview-v1`. Keys are absolute URLs:

| Key | Value |
|---|---|
| `<realm>__manifest` | JSON `{ "version": number, "entity": Entity }` |
| `<realm>content/contents/<hash>` | the raw bytes of one content file, one entry per distinct hash |

```jsonc
// Entity: what the engine expects from POST /content/entities/active
{
  "id": "b64-…",
  "type": "scene",
  "pointers": ["4,-2", "5,-2"],
  "timestamp": 1700000000000,
  "content": [{ "file": "bin/game.js", "hash": "b64-…" }],
  "metadata": { /* scene.json: the engine needs main, scene.base, scene.parcels */ }
}
```

The worker ignores `version` and any other manifest field. A manifest whose entity has no `b64-`
id, `pointers` array or `content` array is treated as absent. Write every content entry before the
manifest, so the engine never sees a manifest pointing at bytes that are not there yet.

## Ids

The scheme of sdk-commands' preview server (`b64HashingFunction`,
`b64ContentVersionedHashingFunction`), which is what makes the engine treat the scene as local
(no disk cache, no `X-IPFS` cache, hot reload):

- entity id: `'b64-' + base64("<projectRoot>-<machineId>")`
- content hash: `'b64-' + base64("<projectRoot>/<file>\0<version>-<machineId>")`, `<version>`
  digits only and changed whenever the file changes (sdk-commands uses the mtime). The unversioned
  form `base64("<projectRoot>/<file>-<machineId>")` is also accepted.

`base64` is the standard alphabet with padding, over the UTF-8 bytes. The engine decodes these
(`crates/ipfs/src/lib.rs` `b64_split_at_key`) to recover the project root, so `<file>` must be the
path the content entry names. The entity id must not contain `/`: keep `<projectRoot>` and
`<machineId>` to `[A-Za-z0-9/_.-]`. Content hashes may contain `/` and `+`. Only `b64-` hashes are
served.

Make the entity id unique per creator and project (e.g. a random `<machineId>`): the engine keys
its scene room and presence partition on it.

## Routes

| Request | Answer |
|---|---|
| `GET <realm>about` | realm description, below |
| `POST <realm>content/entities/active`, body `{ "pointers": [...] }` | `[entity]` when a requested pointer is one of the entity's, else `[]`; plus what a catalyst answers for the pointers that are not parcels |
| `GET <realm>content/contents/<entity id>` | the entity JSON (the engine loads the scene entity this way) |
| `GET <realm>content/contents/<hash>` | the stored bytes, 404 when missing |
| `GET <realm>scene.json` | `entity.metadata` (asked for by the engine's presence service on a local realm) |
| anything else, incl. `GET <realm>scenes` | 404 |

The engine resolves wearables and emotes through its realm's `entities/active` too, by urn. Those
pointers (anything not shaped `x,y`) are forwarded to `https://peer.decentraland.org/content/entities/active`
(`peer.decentraland.zone` when the page is on decentraland.zone) and the answer is merged in, so the
avatar has a body on a preview realm. The files of those entities never pass through the realm: the
engine loads them from its own catalyst. If the catalyst cannot be reached the answer is a 502, which
the engine retries; an empty list would mark the wearables missing for good. A page launched with
another `?baseDomain=` or `?catalyst=` still gets this default catalyst: the worker does not see
the page's params.

If `preview_realm.js` failed to load, the worker still installs and answers every preview URL 503.

```json
{
  "healthy": true,
  "acceptingUsers": true,
  "configurations": {
    "networkId": 0,
    "globalScenesUrn": [],
    "scenesUrn": [],
    "localSceneParcels": ["4,-2", "5,-2"],
    "realmName": "LocalPreview"
  },
  "content": { "healthy": true, "publicUrl": "<realm>content" },
  "comms": { "healthy": true, "protocol": "v3", "fixedAdapter": "offline:offline" }
}
```

`localSceneParcels` is the entity's `pointers`. There is no `lambdas` endpoint.

Every response carries `X-Content-Type-Options: nosniff`, `Content-Security-Policy: sandbox`,
`Cross-Origin-Resource-Policy: same-origin` and `Cache-Control: no-store`, with
`Content-Type: application/json` for `about`, `scene.json` and `entities/active` and
`application/octet-stream` for everything else. Headers stored with a cache entry are dropped. A
navigation to a preview URL downloads; it never renders.

## Launch

```
<PAGE_DIR>?realm=<encodeURIComponent(PAGE_DIR + 'preview/' + projectId)>&position=<scene.base>&preview
```

- `realm` is absolute, with no trailing slash, and exactly this string: the engine appends `/about`
  to it, and preview mode only engages while the realm it was launched with is the one it is on.
- `position` is the scene's base parcel.
- `preview` (a flag: present = on) turns on the engine's preview mode: no failed-asset backoff, `isPreview` in the
  scene's realm info, realm changes disabled. The engine also retries a websocket to the realm URL
  every 5 s, which nothing answers.
- The page must be controlled by the service worker (the app reloads once on a first visit to get
  there); a hard reload bypasses it and the realm is then unreachable.

## Hot reload

After rewriting the store:

```js
window.engine_console_command('/reload <entityId>')
```

Always with the id: a bare `/reload` respawns every scene, the HUD's included.

The engine respawns that one scene: it reads the entity again from `content/contents/<entity id>`
and then the bundle under its new hash. It does not ask `entities/active` again (it keeps the
parcels it already resolved), so a change to the scene's parcels needs a new launch.
`react-web/e2e/preview-realm.gate.spec.ts` runs this whole loop in the real engine.

## Storage

The routes `sdk-commands start` serves a scene's server (`storage-service.js`), per realm, for an
authoritative preview's scene server, which the editor host starts in the tab
(`engine/headless.js`, react-web `features/editorHost/host/host.ts`):

| Request | Answer |
|---|---|
| `GET <realm>values?prefix=&limit=&offset=` | `{ "data": [{ "key", "value" }], "pagination": { "offset", "total" } }` |
| `GET / PUT / DELETE <realm>values/<key>` | `{ "value" }` (404 when missing) / `{ "value" }` / 204 |
| `GET <realm>players/<address>/values?…`, `GET / PUT / DELETE …/values/<key>` | the same, for that address alone |
| `GET / PUT / DELETE <realm>env/<key>` | `{ "value" }` (404 when missing) / 204 / 204 |
| `DELETE <realm>values` or `…/players/<address>/values`, with `X-Confirm-Delete-All: true` | clears that scope, 204 (404 without the header) |

PUT bodies are `{ "value": … }`. Everything is kept in cache `dcl-editor-storage-v1` under
`<realm>__storage`, one JSON `{ env, world, players }` per realm like the dev server's
`server-storage.json`; writes to a realm are applied one at a time. It survives reloads and is never
synced or logged.

Who may call them, as the native preview trusts its own server: the worker answers a storage route
only when the request's service worker client is

- a window on this origin: the page (the editor's Storage tab, through the host's
  `previewStorageFetch`) and the scene server's hidden `headless.html` frame, which is the page's
  own code; or
- a scene server's sandbox: `engine/pkg/sandbox_worker.bundle.js?server`, the url
  `engine/sandbox_host.js` gives the sandboxes of a `role: "server"` instance (`headless.js`).

Anything else gets a 403 with no body: the client's sandboxes (`sandbox_worker.bundle.js` with no
query) and so the scene's client copy, portables and smart wearables, other workers, and requests
with no client. Scene code cannot pass for the server: `sandbox_worker.js` deletes `Worker` and
`SharedWorker` before any scene code runs, and a module worker has no `importScripts`, so a scene
has no way to start a client under another url. The server copy's realm info says `isPreview`,
which is what points the SDK's `Storage` and `EnvVar` at the realm.

## Editor scene

The worker answers a second, simpler realm for the editor package's own super-user scene
(react-web `features/editorHost/host/editorScene.ts`): `<PAGE_DIR>editor-scene/<entityId>/`, from
cache `dcl-editor-scene-v1`. The page writes `<realm>contents/<hash>` for the entity and each file
it lists, every one checked against its hash first, then `<realm>about` (listing the entity in
`scenesUrn`, `baseUrl` = `<realm>contents/`). The worker serves `GET about` and
`GET contents/<hash>` as stored, with the headers above; anything else is a 404. Only the
current entity is kept: staging another one deletes the rest.

## Security

- Routes read only keys under their own realm prefix, only from `dcl-editor-preview-v1`. The one
  request the worker makes is the `entities/active` forward: a fixed catalyst url, the non-parcel
  pointers and nothing else.
- Scene code cannot write the store: `engine/sandbox_worker.js` deletes `caches` (and `indexedDB`,
  `navigator.storage`) from the scene worker before any scene code runs, and the service worker
  itself never writes this cache. At most a scene can read preview URLs with `fetch`.
- Nor can scene code read it, though its realm info names the preview's url: every route here and
  under `editor-scene/` answers 403 to a request whose service worker client is a scene's sandbox
  (`engine/pkg/sandbox_worker.bundle.js`, a server's `?server` ones included), or that has no
  client. That covers its `fetch`, XHR and `import()` alike. A server loads its scene through its
  engine, not its sandboxes. Storage routes have their own rule (Storage, above).
- Project bytes are same-origin with the page, hence the headers above.
