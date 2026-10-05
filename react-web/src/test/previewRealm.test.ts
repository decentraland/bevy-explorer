// The editor's preview realm as the service worker answers it (deploy/web/PREVIEW_REALM.md):
// what the engine fetches from a realm, served from the preview cache alone.
import { describe, expect, it, vi } from 'vitest'
import '../../../deploy/web/preview_realm.js'

interface PreviewStore {
  match: (key: string) => Promise<Response | undefined>
}
interface StorageStore extends PreviewStore {
  put: (key: string, value: Response) => Promise<void>
}
// the service worker client behind a request, as service_worker.js resolves it
interface Client {
  url: string
  type: 'window' | 'worker' | 'sharedworker'
}

declare global {
  var dclPreviewRealm: {
    handle: (request: Request, previewRoot: string, store: PreviewStore, storage?: StorageStore, client?: Client) => Promise<Response>
    handleEditorScene: (request: Request, root: string, store: PreviewStore, client?: Client) => Promise<Response>
  }
}

// a sub-path scope, as in production
const ROOT = 'https://play.example/bevy-web/preview/'
const REALM = `${ROOT}my-scene/`
const ENTITY_ID = 'b64-L215LXNjZW5lLXdlYg=='
const GAME_HASH = 'b64-L215LXNjZW5lL2Jpbi9nYW1lLmpzADE3MDAtd2Vi'

const entity = {
  id: ENTITY_ID,
  type: 'scene',
  pointers: ['4,-2', '5,-2'],
  timestamp: 1700,
  content: [{ file: 'bin/game.js', hash: GAME_HASH }],
  metadata: { main: 'bin/game.js', scene: { base: '4,-2', parcels: ['4,-2', '5,-2'] } }
}

function storeOf(entries: Record<string, string>): PreviewStore {
  return {
    match: async (key) => (key in entries ? new Response(entries[key], { headers: { 'Content-Type': 'text/html' } }) : undefined)
  }
}

const manifest = JSON.stringify({ version: 3, entity })
const store = storeOf({
  [`${REALM}__manifest`]: manifest,
  [`${REALM}content/contents/${GAME_HASH}`]: '<script>game()</script>',
  // held, but outside the contract: neither may be served
  [`${REALM}content/contents/bafkreiplaincid`]: 'not a local id',
  [`${ROOT}Bad_Id/__manifest`]: manifest
})

// the service worker's client for the request: the page (and the engine on it), or a scene's sandbox
const PAGE_DIR = 'https://play.example/bevy-web/'
const PAGE: Client = { url: PAGE_DIR, type: 'window' }
const SANDBOX: Client = { url: `${PAGE_DIR}engine/pkg/sandbox_worker.bundle.js`, type: 'worker' }
// a scene server's sandbox (engine/sandbox_host.js, role 'server')
const SERVER_SANDBOX: Client = { url: `${SANDBOX.url}?server`, type: 'worker' }

const get = (url: string, client: Client | null = PAGE): Promise<Response> =>
  dclPreviewRealm.handle(new Request(url), ROOT, store, undefined, client ?? undefined)
const active = (pointers: string[]): Promise<Response> =>
  dclPreviewRealm.handle(
    new Request(`${REALM}content/entities/active`, { method: 'POST', body: JSON.stringify({ pointers }) }),
    ROOT,
    store,
    undefined,
    PAGE
  )

describe('preview realm', () => {
  it('describes a one-scene realm whose content server is the realm itself, comms offline', async () => {
    const res = await get(`${REALM}about`)
    expect(res.status).toBe(200)
    expect(res.headers.get('Content-Type')).toBe('application/json')
    const about = await res.json()
    // the fields the engine's ServerAbout requires (crates/ipfs/src/lib.rs)
    expect(about.content).toEqual({ healthy: true, publicUrl: `${REALM}content` })
    expect(about.comms).toEqual({ healthy: true, protocol: 'v3', fixedAdapter: 'offline:offline' })
    expect(about.configurations.scenesUrn).toEqual([])
    expect(about.configurations.localSceneParcels).toEqual(['4,-2', '5,-2'])
  })

  it('answers the realm the engine reaches by appending to a trailing-slash realm url', async () => {
    expect((await get(`${REALM}/about`)).status).toBe(200)
  })

  it('returns the scene for any of its pointers, nothing for other parcels, and asks a catalyst for the rest', async () => {
    const eyes = { id: 'bafkreieyes', pointers: ['urn:decentraland:off-chain:base-avatars:eyes_00'] }
    const remoteScene = { id: 'bafkreiscene', type: 'scene', pointers: ['5,-2'] }
    const catalyst = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => new Response(JSON.stringify([eyes, remoteScene])))
    expect(await (await active(['9,9', '5,-2'])).json()).toEqual([entity])
    expect(await (await active(['9,9'])).json()).toEqual([])
    expect(catalyst).not.toHaveBeenCalled()

    // the engine resolves the avatar's wearables through its realm; a scene there is not the project's
    expect(await (await active(['5,-2', eyes.pointers[0]])).json()).toEqual([entity, eyes])
    const [url, init] = catalyst.mock.lastCall!
    expect(url).toBe('https://peer.decentraland.org/content/entities/active')
    expect(JSON.parse(init!.body as string)).toEqual({ pointers: eyes.pointers })

    // unreachable: an error the engine retries, not "no such wearable"
    catalyst.mockRejectedValue(new Error('offline'))
    expect((await active(eyes.pointers)).status).toBe(502)
    catalyst.mockRestore()
  })

  it('serves stored bytes and the entity by id as inert downloads', async () => {
    const file = await get(`${REALM}content/contents/${GAME_HASH}`)
    expect(await file.text()).toBe('<script>game()</script>')
    // the stored response's own headers never reach the client
    expect(Object.fromEntries(file.headers)).toEqual({
      'cache-control': 'no-store',
      'content-security-policy': 'sandbox',
      'content-type': 'application/octet-stream',
      'cross-origin-resource-policy': 'same-origin',
      'x-content-type-options': 'nosniff'
    })

    // the engine loads the scene entity through the contents route
    expect(await (await get(`${REALM}content/contents/${ENTITY_ID}`)).json()).toEqual(entity)
  })

  it('never answers a scene’s own requests, whose realm info names the preview', async () => {
    for (const path of ['about', `content/contents/${GAME_HASH}`, `content/contents/${ENTITY_ID}`]) {
      // a client the worker cannot name is refused too
      for (const client of [SANDBOX, null]) {
        const res = await get(`${REALM}${path}`, client)
        expect([path, client, res.status]).toEqual([path, client, 403])
      }
    }
    // the server loads its scene through its engine: its sandboxes get no files either
    expect((await get(`${REALM}content/contents/${GAME_HASH}`, SERVER_SANDBOX)).status).toBe(403)
    const editorRoot = `${PAGE_DIR}editor-scene/`
    const editorStore = storeOf({ [`${editorRoot}bafkreiabc/about`]: '{}' })
    const editorAbout = (client: Client): Promise<Response> =>
      dclPreviewRealm.handleEditorScene(new Request(`${editorRoot}bafkreiabc/about`), editorRoot, editorStore, client)
    expect([(await editorAbout(PAGE)).status, (await editorAbout(SANDBOX)).status]).toEqual([200, 403])
  })

  it('opens a realm’s storage to the page and its scene server, to no scene', async () => {
    const entries = new Map<string, string>()
    const storage: StorageStore = {
      match: async (key) => (entries.has(key) ? new Response(entries.get(key)) : undefined),
      put: async (key, value) => void entries.set(key, await value.text())
    }
    const call = (path: string, client: Client | undefined, init?: RequestInit): Promise<Response> =>
      dclPreviewRealm.handle(new Request(`${REALM}${path}`, init), ROOT, store, storage, client)

    const set = await call('values/score', SERVER_SANDBOX, { method: 'PUT', body: JSON.stringify({ value: 3 }) })
    expect(await set.json()).toEqual({ value: 3 })
    expect(await (await call('values/score', PAGE)).json()).toEqual({ value: 3 })
    expect(await (await call('players/0xab/values', SERVER_SANDBOX)).json()).toEqual({ data: [], pagination: { offset: 0, total: 0 } })

    // the scene's client copy, a portable or a smart wearable, and a client the worker cannot name
    for (const client of [SANDBOX, undefined]) {
      for (const path of ['values/score', 'values', 'players/0xab/values', 'env/API_KEY']) {
        const res = await call(path, client)
        expect([path, client, res.status, await res.text()]).toEqual([path, client, 403, ''])
      }
      expect((await call('values/score', client, { method: 'DELETE' })).status).toBe(403)
    }
    expect(await (await call('values/score', PAGE)).json()).toEqual({ value: 3 })
  })

  it('answers 404, never the network, for everything it does not hold', async () => {
    const missing = [
      `${REALM}content/contents/b64-bm90LXN0b3JlZA==`,
      `${REALM}content/contents/bafkreiplaincid`,
      `${REALM}__manifest`,
      // the engine probes /scenes and takes a 404 as "no scene list"
      `${REALM}scenes`,
      `${ROOT}other-scene/about`,
      `${ROOT}Bad_Id/about`
    ]
    for (const url of missing) {
      const res = await get(url)
      expect([url, res.status]).toEqual([url, 404])
      expect(res.headers.get('Content-Security-Policy')).toBe('sandbox')
    }
  })
})
