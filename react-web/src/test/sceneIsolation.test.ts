// What the web engine's scripts let a scene reach: the sandbox worker hands the system ops to the
// super-user scene only, and the service worker's asset cache keeps each origin's responses apart.
import { readFileSync } from 'node:fs'
import { describe, expect, it, vi } from 'vitest'

const read = (path: string): string => readFileSync(new URL(path, import.meta.url), 'utf8')

interface SceneScope {
  Deno: { core: { ops: Record<string, unknown> } }
}

// the worker's script, run against a stand-in global; it exports nothing, so the scope is read back
function sandboxOps(isSuper: boolean, opNames: string[]): Record<string, unknown> {
  const src = read('../../../deploy/web/engine/sandbox_worker.js').replace(/^import .*$/m, '')
  const quiet = { log: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(), trace: vi.fn() }
  const load = new Function('self', 'console', `${src}\nreturn { createJsContext, scope: () => jsContext }`) as (
    self: object,
    console: object
  ) => { createJsContext: (wasmApi: object, context: object) => void; scope: () => SceneScope }
  const worker = load({ navigator: {}, BroadcastChannel: class {} }, quiet)
  const wasmApi = Object.fromEntries([...opNames, 'op_crdt_send_to_renderer'].map((name) => [name, vi.fn()]))
  worker.createJsContext({ ...wasmApi, is_super: () => isSuper }, { get_scene_title: () => 'scene' })
  return worker.scope().Deno.core.ops
}

describe('scene isolation on web', () => {
  it('gives every op native registers for the super-user scene to that scene only', () => {
    const native = /if super_user \{\s*vec!\[([^\]]*)\]/.exec(read('../../../crates/dcl_deno/src/js/op_wrappers/system_api.rs'))
    const systemOps = [...(native?.[1] ?? '').matchAll(/(op_\w+)\(\)/g)].map((m) => m[1])
    expect(systemOps).toContain('op_kernel_fetch_headers')

    const scene = sandboxOps(false, systemOps)
    expect(systemOps.filter((op) => op in scene)).toEqual([])
    expect('op_crdt_send_to_renderer' in scene).toBe(true)
    expect(systemOps.filter((op) => !(op in sandboxOps(true, systemOps)))).toEqual([])
  })

  it("never answers one origin's asset request with another origin's cached response", async () => {
    const listeners: Record<string, (event: unknown) => void> = {}
    const store = new Map<string, Response>()
    const cache = {
      match: async (key: string) => store.get(key)?.clone(),
      put: async (key: string, res: Response) => void store.set(key, res)
    }
    const network = vi.fn(async (req: Request) => new Response(new URL(req.url).origin))
    const self = { addEventListener: (type: string, fn: (event: unknown) => void) => (listeners[type] = fn), registration: { scope: 'https://play.example/' } }
    const load = new Function('self', 'caches', 'fetch', 'console', read('../../../deploy/web/service_worker.js')) as (
      self: object,
      caches: object,
      fetch: object,
      console: object
    ) => void
    load(self, { open: async () => cache }, network, { log: vi.fn(), warn: vi.fn() })

    const asset = async (url: string): Promise<string> => {
      let reply: Promise<Response> | undefined
      listeners.fetch({ request: new Request(url, { headers: { 'X-IPFS': '1' } }), respondWith: (r: Promise<Response>) => (reply = r) })
      return (await reply!).text()
    }
    expect(await asset('https://evil.example/contents/bafyhash')).toBe('https://evil.example')
    expect(await asset('https://peer.decentraland.org/contents/bafyhash')).toBe('https://peer.decentraland.org')
  })
})
