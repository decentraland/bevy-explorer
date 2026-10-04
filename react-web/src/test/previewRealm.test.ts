// The editor's preview realm as the service worker answers it (deploy/web/PREVIEW_REALM.md):
// what the engine fetches from a realm, served from the preview cache alone.
import { describe, expect, it, vi } from 'vitest'
import '../../../deploy/web/preview_realm.js'
import { grantStorage, revokeStorage } from '../features/editorHost/host/previewStorage'

interface PreviewStore {
  match: (key: string) => Promise<Response | undefined>
}
interface StorageStore extends PreviewStore {
  put: (key: string, value: Response) => Promise<void>
}

declare global {
  var dclPreviewRealm: {
    handle: (request: Request, previewRoot: string, store: PreviewStore, storage?: StorageStore) => Promise<Response>
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
const PAGE = 'https://play.example/bevy-web/'
const SANDBOX = 'https://play.example/bevy-web/engine/pkg/sandbox_worker.bundle.js'

const get = (url: string, client: string | null = PAGE): Promise<Response> => dclPreviewRealm.handle(new Request(url), ROOT, store, client ?? undefined)
const active = (pointers: string[]): Promise<Response> =>
  dclPreviewRealm.handle(
    new Request(`${REALM}content/entities/active`, { method: 'POST', body: JSON.stringify({ pointers }) }),
    ROOT,
    store,
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
    const editorRoot = `${PAGE}editor-scene/`
    const editorStore = storeOf({ [`${editorRoot}bafkreiabc/about`]: '{}' })
    const editorAbout = (client: string): Promise<Response> =>
      dclPreviewRealm.handleEditorScene(new Request(`${editorRoot}bafkreiabc/about`), editorRoot, editorStore, client)
    expect([(await editorAbout(PAGE)).status, (await editorAbout(SANDBOX)).status]).toEqual([200, 403])
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

  describe('storage for the in-tab scene server', () => {
    // what the page keeps in dcl-editor-storage-v1 (react-web host/previewStorage.ts)
    function storageOf(access: { realm: string; token: string }): StorageStore {
      const entries = new Map<string, string>([[`${access.realm}/__server`, JSON.stringify({ token: access.token })]])
      return {
        match: async (key) => (entries.has(key) ? new Response(entries.get(key)) : undefined),
        put: async (key, value) => void entries.set(key, await value.text())
      }
    }
    const realm = REALM.slice(0, -1)
    const call = (storage: StorageStore, path: string, init: RequestInit & { token?: string } = {}): Promise<Response> =>
      dclPreviewRealm.handle(
        new Request(`${REALM}${path}`, { ...init, headers: init.token == null ? {} : { 'x-dcl-local-server': init.token } }),
        ROOT,
        store,
        storage
      )
    const put = (value: unknown): RequestInit => ({ method: 'PUT', body: JSON.stringify({ value }) })

    it('answers the routes the dev server serves a scene server, as it does', async () => {
      const storage = storageOf({ realm, token: 't1' })
      const as = { token: 't1' }
      expect(await (await call(storage, 'values/score', { ...as, ...put({ best: 3 }) })).json()).toEqual({ value: { best: 3 } })
      expect(await (await call(storage, 'values/scene%2Fname', { ...as, ...put('arena') })).json()).toEqual({ value: 'arena' })
      expect(await (await call(storage, 'values/score', as)).json()).toEqual({ value: { best: 3 } })
      expect(await (await call(storage, 'values?prefix=sc&limit=1&offset=1', as)).json()).toEqual({
        data: [{ key: 'scene/name', value: 'arena' }],
        pagination: { offset: 1, total: 2 }
      })
      expect((await call(storage, 'values/score', { ...as, method: 'DELETE' })).status).toBe(204)
      expect((await call(storage, 'values/score', as)).status).toBe(404)

      // a player's values are theirs alone
      await call(storage, 'players/0xab/values/coins', { ...as, ...put(5) })
      expect(await (await call(storage, 'players/0xab/values/coins', as)).json()).toEqual({ value: 5 })
      expect((await call(storage, 'players/0xcd/values/coins', as)).status).toBe(404)
      expect(await (await call(storage, 'players/0xab/values', as)).json()).toEqual({
        data: [{ key: 'coins', value: 5 }],
        pagination: { offset: 0, total: 1 }
      })
      expect(await (await call(storage, 'values', as)).json()).toEqual({ data: [{ key: 'scene/name', value: 'arena' }], pagination: { offset: 0, total: 1 } })

      expect((await call(storage, 'env/API_KEY', { ...as, ...put('k-1') })).status).toBe(204)
      expect(await (await call(storage, 'env/API_KEY', as)).json()).toEqual({ value: 'k-1' })
      expect((await call(storage, 'env/API_KEY', { ...as, method: 'DELETE' })).status).toBe(204)
      expect((await call(storage, 'env/API_KEY', as)).status).toBe(404)
    })

    it('opens a realm’s storage only to the token the page keeps for that realm', async () => {
      const storage = storageOf({ realm, token: 't1' })
      await call(storage, 'env/API_KEY', { token: 't1', ...put('k-1') })
      for (const token of [undefined, 't2']) {
        for (const path of ['env/API_KEY', 'values', 'players/0xab/values']) {
          const res = await call(storage, path, { token })
          expect([path, token, res.status, await res.text()]).toEqual([path, token, 403, ''])
        }
        expect((await call(storage, 'values/x', { token, ...put(1) })).status).toBe(403)
      }
      // the same token does not open another project's realm
      const other = await dclPreviewRealm.handle(
        new Request(`${ROOT}other-scene/env/API_KEY`, { headers: { 'x-dcl-local-server': 't1' } }),
        ROOT,
        store,
        storage
      )
      expect(other.status).toBe(403)
    })

    it('keeps each tab’s grant open while another tab previews and leaves', async () => {
      const entries = new Map<string, string>()
      const shared: StorageStore & { delete: (key: string) => Promise<boolean> } = {
        match: async (key) => (entries.has(key) ? new Response(entries.get(key)) : undefined),
        put: async (key, value) => void entries.set(key, await value.text()),
        delete: async (key) => entries.delete(key)
      }
      vi.stubGlobal('caches', { open: async () => shared })
      try {
        const read = async (project: string, token: string): Promise<number> =>
          (
            await dclPreviewRealm.handle(
              new Request(`${ROOT}${project}/values`, { headers: { 'x-dcl-local-server': token } }),
              ROOT,
              store,
              shared
            )
          ).status
        const a = (await grantStorage(`${ROOT}scene-a`))!
        const b = (await grantStorage(`${ROOT}scene-b`))!
        expect([await read('scene-a', a), await read('scene-b', b)]).toEqual([200, 200])
        await revokeStorage(`${ROOT}scene-b`, b)
        expect([await read('scene-a', a), await read('scene-b', b)]).toEqual([200, 403])
        // a later grant for the same realm is not closed by an earlier tab leaving
        const a2 = (await grantStorage(`${ROOT}scene-a`))!
        await revokeStorage(`${ROOT}scene-a`, a)
        expect(await read('scene-a', a2)).toBe(200)
      } finally {
        vi.unstubAllGlobals()
      }
    })

    it('keeps an address named like a prototype key to its own realm', async () => {
      const mine = storageOf({ realm, token: 't1' })
      await call(mine, 'players/__proto__/values/0xvictim', { token: 't1', ...put({ score: 999 }) })
      try {
        expect(({} as Record<string, unknown>)['0xvictim']).toBeUndefined()
        const otherRealm = `${ROOT}other-scene`
        const theirs = storageOf({ realm: otherRealm, token: 't2' })
        const read = await dclPreviewRealm.handle(
          new Request(`${otherRealm}/players/0xvictim/values/score`, { headers: { 'x-dcl-local-server': 't2' } }),
          ROOT,
          store,
          theirs
        )
        expect(read.status).toBe(404)
        expect(await (await call(mine, 'players/__proto__/values/0xvictim', { token: 't1' })).json()).toEqual({ value: { score: 999 } })
      } finally {
        delete (Object.prototype as Record<string, unknown>)['0xvictim']
      }
    })
  })
})
