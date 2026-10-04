// The service worker's asset cache: what it fetches stays under the url it came from; only an
// entry under the shared key (the hash alone, which the engine writes once it has checked the bytes
// against that hash) answers for every content server.
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, vi } from 'vitest'

const SHARED = 'https://shared.invalid'

// the worker's script, run against a stand-in global; `seeded` is what the cache already holds
function serviceWorker(seeded: Record<string, string> = {}) {
  const listeners: Record<string, (event: unknown) => void> = {}
  const store = new Map<string, Response>(Object.entries(seeded).map(([key, body]) => [key, new Response(body)]))
  const cache = {
    match: async (key: string) => store.get(key)?.clone(),
    put: async (key: string, res: Response) => void store.set(key, res),
    delete: async (key: string) => store.delete(key)
  }
  const network = vi.fn(async (req: Request) => new Response(new URL(req.url).origin))
  const self = {
    addEventListener: (type: string, fn: (event: unknown) => void) => (listeners[type] = fn),
    registration: { scope: 'https://play.example/' }
  }
  const source = readFileSync(resolve(__dirname, '../../../deploy/web/service_worker.js'), 'utf8')
  const load = new Function('self', 'caches', 'fetch', 'console', source) as (
    self: object,
    caches: object,
    fetch: object,
    console: object
  ) => void
  load(self, { open: async () => cache }, network, { log: vi.fn(), warn: vi.fn() })

  const asset = async (url: string, method = 'GET'): Promise<{ body: string; cached: boolean }> => {
    let reply: Promise<Response> | undefined
    listeners.fetch({
      request: new Request(url, { method, headers: { 'X-IPFS': '1' } }),
      respondWith: (r: Promise<Response>) => (reply = r)
    })
    const response = await reply!
    return { body: await response.text(), cached: response.headers.has('X-IPFS-Cached') }
  }
  return { asset, network, keys: () => [...store.keys()] }
}

describe('service worker asset cache', () => {
  const hash = 'bafkreiexample'
  const path = `/contents/${hash}`

  it('keeps what it fetches with the origin that served it', async () => {
    const { asset, network, keys } = serviceWorker()
    expect(await asset(`https://evil.example${path}`)).toEqual({ body: 'https://evil.example', cached: false })
    expect(await asset(`https://peer.example${path}`)).toEqual({ body: 'https://peer.example', cached: false })
    expect(await asset(`https://peer.example${path}`)).toEqual({ body: 'https://peer.example', cached: true })
    expect(network).toHaveBeenCalledTimes(2)
    expect(keys().sort()).toEqual([`https://evil.example${path}`, `https://peer.example${path}`])
  })

  it('answers every origin from the shared entry, dropping the origin\'s own copy', async () => {
    const { asset, network, keys } = serviceWorker({
      [`${SHARED}/${hash}`]: 'checked',
      [`https://evil.example${path}`]: 'unchecked'
    })
    expect(await asset(`https://peer.example${path}`)).toEqual({ body: 'checked', cached: true })
    // whatever path the server keeps its content under
    expect(await asset(`https://worlds.example/world${path}`)).toEqual({ body: 'checked', cached: true })
    expect(await asset(`https://evil.example${path}`)).toEqual({ body: 'checked', cached: true })
    expect(network).not.toHaveBeenCalled()
    // and drops that origin's own copy
    expect(keys()).toEqual([`${SHARED}/${hash}`])
  })

  it('keys a request by where its path ends, not by what follows a `#`', async () => {
    const { asset, keys } = serviceWorker()
    await asset(`https://evil.example/contents/bafkreitarget#${path}`)
    expect(keys()).toEqual(['https://evil.example/contents/bafkreitarget'])
  })

  it('neither stores nor answers anything but a GET', async () => {
    const { asset, network, keys } = serviceWorker()
    expect(await asset(`https://peer.example${path}`, 'HEAD')).toEqual({ body: 'https://peer.example', cached: false })
    expect(keys()).toEqual([])
    expect(await asset(`https://peer.example${path}`)).toEqual({ body: 'https://peer.example', cached: false })
    expect(await asset(`https://peer.example${path}`, 'HEAD')).toEqual({ body: 'https://peer.example', cached: false })
    expect(network).toHaveBeenCalledTimes(3)
  })
})
