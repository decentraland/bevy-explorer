// The editor's preview realm as the service worker answers it (deploy/web/PREVIEW_REALM.md):
// what the engine fetches from a realm, served from the preview cache alone.
import { describe, expect, it } from 'vitest'
import '../../../deploy/web/preview_realm.js'

interface PreviewStore {
  match: (key: string) => Promise<Response | undefined>
}

declare global {
  var dclPreviewRealm: {
    handle: (request: Request, previewRoot: string, store: PreviewStore) => Promise<Response>
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

const get = (url: string): Promise<Response> => dclPreviewRealm.handle(new Request(url), ROOT, store)
const active = (pointers: string[]): Promise<Response> =>
  dclPreviewRealm.handle(
    new Request(`${REALM}content/entities/active`, { method: 'POST', body: JSON.stringify({ pointers }) }),
    ROOT,
    store
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

  it('returns the scene for any of its pointers and nothing for the rest', async () => {
    expect(await (await active(['9,9', '5,-2'])).json()).toEqual([entity])
    expect(await (await active(['9,9', 'urn:decentraland:off-chain:base-avatars:eyes_00'])).json()).toEqual([])
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
