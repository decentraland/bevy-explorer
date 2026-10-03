// What the web engine's scripts let a scene reach: ordinary scenes get only the scene ops (every
// other op goes to the super-user scene alone), and the service worker's asset cache keeps each origin's responses apart.
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
  const wasmApi = Object.fromEntries(opNames.map((name) => [name, vi.fn()]))
  worker.createJsContext({ ...wasmApi, is_super: () => isSuper }, { get_scene_title: () => 'scene' })
  return worker.scope().Deno.core.ops
}

describe('scene isolation on web', () => {
  it('gives ordinary scenes only the scene ops, and the super-user scene every op', () => {
    const opsIn = (src: string): string[] => [...src.matchAll(/pub (?:async )?fn (op_\w+)/g)].map((m) => m[1])
    const dir = '../../../crates/dcl_wasm/src/inner/'
    const systemOps = opsIn(read(`${dir}op_wrappers/system_api.rs`))
    const sceneOps = ['adaption_layer_helper', 'comms', 'engine', 'ethereum_controller', 'events', 'fetch', 'player', 'portables', 'restricted_actions', 'runtime', 'testing', 'user_identity']
      .flatMap((name) => opsIn(read(`${dir}op_wrappers/${name}.rs`)))
      .concat(opsIn(read(`${dir}mod.rs`)), opsIn(read(`${dir}local_storage.rs`)))
    expect(systemOps).toContain('op_kernel_fetch_headers')

    const all = [...systemOps, ...sceneOps]
    const scene = sandboxOps(false, all)
    expect(Object.getOwnPropertyNames(scene).sort()).toEqual([...sceneOps].sort())
    expect(all.filter((op) => !(op in sandboxOps(true, all)))).toEqual([])
    expect('op_new_unlisted' in sandboxOps(false, [...all, 'op_new_unlisted'])).toBe(false)
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
