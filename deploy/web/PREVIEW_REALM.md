# Preview realm (contract v0)

A Decentraland realm served by `service_worker.js` out of Cache Storage, so a scene built in the
browser loads in the engine with no server. The page (the scene editor) writes the store; the
worker (`preview_realm.js`) only reads it.

## Where

- `<PAGE_DIR>` is the directory the service worker is registered at (its scope).
- A realm lives at `<PAGE_DIR>preview/<projectId>/`, `projectId` matching `/^[a-z0-9][a-z0-9-]{0,63}$/`.
- Every request under `<PAGE_DIR>preview/` is answered by the worker and never reaches the network
  or `ipfs-path-cache-v1`. Anything it does not hold is a 404.

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
| `POST <realm>content/entities/active`, body `{ "pointers": [...] }` | `[entity]` when a requested pointer is one of the entity's, else `[]` |
| `GET <realm>content/contents/<entity id>` | the entity JSON (the engine loads the scene entity this way) |
| `GET <realm>content/contents/<hash>` | the stored bytes, 404 when missing |
| `GET <realm>scene.json` | `entity.metadata` (asked for by the engine's presence service on a local realm) |
| anything else, incl. `GET <realm>scenes` | 404 |

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

## Security

- Routes read only keys under their own realm prefix, only from `dcl-editor-preview-v1`.
- Scene code cannot write the store: `engine/sandbox_worker.js` deletes `caches` (and `indexedDB`,
  `navigator.storage`) from the scene worker before any scene code runs, and the service worker
  itself never writes this cache. At most a scene can read preview URLs with `fetch`.
- Project bytes are same-origin with the page, hence the headers above.
