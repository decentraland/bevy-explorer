// The editor package's super-user scene (features/editorHost/host/editorScene.ts): with a pinned
// entity id the page spawns only bytes that hash to it, and serves them itself.
import { afterAll, describe, expect, it, vi } from 'vitest'
import { contentCid } from '../features/editorHost/host/cid'
import { loadEditor, type DclEditorHostV1, type EditorHostDeps } from '../features/editorHost/host/host'
import { PAGE_DIR } from '../lib/publicUrl'

const utf8 = (text: string): Uint8Array<ArrayBuffer> => new TextEncoder().encode(text)
const BASE = 'https://cdn.example/editor/1.0.0/'

describe('editor scene', () => {
  const stored = new Map<string, Response>()
  vi.stubGlobal('caches', {
    open: async () => ({
      keys: async () => [...stored.keys()].map((url) => new Request(url)),
      delete: async (key: Request) => stored.delete(key.url),
      put: async (url: string, res: Response) => void stored.set(url, res)
    })
  })
  Object.defineProperty(navigator, 'serviceWorker', { configurable: true, value: { controller: {} } })
  afterAll(() => vi.unstubAllGlobals())

  it('spawns only the pinned entity, from the bytes it checked, and refuses swapped ones', async () => {
    const code = utf8('console.log("editor scene")')
    const codeHash = await contentCid(code)
    const entity = utf8(JSON.stringify({ type: 'scene', pointers: [], content: [{ file: 'bin/index.js', hash: codeHash }] }))
    const pinned = await contentCid(entity)
    const served = new Map<string, Uint8Array<ArrayBuffer>>([
      [`${BASE}scene/${pinned}`, entity],
      [`${BASE}scene/${codeHash}`, utf8('console.log("swapped")')]
    ])
    const fetched: string[] = []
    vi.stubGlobal('fetch', async (url: string) => {
      fetched.push(url)
      const body = served.get(url)
      return body ? new Response(body) : new Response(null, { status: 404 })
    })
    const scene = vi.fn<EditorHostDeps['scene']>(async () => {})
    const deps: EditorHostDeps = {
      busSession: '',
      defaultRealm: '',
      engineConsole: async () => '',
      identity: () => ({ address: null, isGuest: true }),
      login: () => null,
      confirmDeployment: async () => false,
      setMode: vi.fn(),
      showCreatePage: vi.fn(),
      travel: async () => {},
      scene
    }
    void loadEditor({ base: BASE, editorJsIntegrity: null, editorSceneEntity: pinned, services: { projects: null, worldsContent: '' } }, PAGE_DIR, deps, false).catch(() => {})
    const host = (window as Window & { __dclEditorHost?: DclEditorHostV1 }).__dclEditorHost!

    await expect(host.spawnEditorScene()).rejects.toThrow(/does not match its hash/)
    expect(scene).not.toHaveBeenCalled()
    expect(stored.size).toBe(0)

    served.set(`${BASE}scene/${codeHash}`, code)
    await expect(host.spawnEditorScene()).resolves.toEqual({ hash: pinned })
    const realm = `${PAGE_DIR}editor-scene/${pinned}`
    expect(scene).toHaveBeenCalledWith('spawn', realm, pinned)
    // the pin names the scene: the package's own `about` is never asked
    expect(fetched.some((url) => url.endsWith('/about'))).toBe(false)
    const about = (await stored.get(`${realm}/about`)!.json()) as { configurations: { scenesUrn: string[] } }
    expect(about.configurations.scenesUrn).toEqual([`urn:decentraland:entity:${pinned}?=&baseUrl=${realm}/contents/`])
    expect(await stored.get(`${realm}/contents/${codeHash}`)!.text()).toBe('console.log("editor scene")')
  })
})
