// The editor hides the HUD, so the space a scene's UI may use is what the editor's own panels leave
// (features/editorHost/host/host.ts setSceneInset).
import { expect, it, vi } from 'vitest'
import { loadEditor, type DclEditorHostV1, type EditorHostDeps } from '../features/editorHost/host/host'
import { PAGE_DIR } from '../lib/publicUrl'

it('passes the space the editor leaves to the engine, and nothing the editor gets wrong', () => {
  vi.stubGlobal('fetch', async () => new Response(null, { status: 404 }))
  Object.defineProperty(navigator, 'serviceWorker', { configurable: true, value: { controller: {} } })
  const setInteractableArea = vi.fn<EditorHostDeps['setInteractableArea']>()
  const deps: EditorHostDeps = {
    busSession: '', defaultRealm: '', engineConsole: async () => '', identity: () => ({ address: null, isGuest: true }),
    login: async () => null, confirmDeployment: async () => false, confirmUndeploy: async () => false,
    setMode: vi.fn(), setInteractableArea, showCreatePage: vi.fn(), travel: async () => {}, scene: vi.fn()
  }
  void loadEditor({ base: 'https://cdn.example/editor/1.0.0/', editorJsIntegrity: null, editorSceneEntity: null,
    services: { projects: null, worldsContent: '' } }, PAGE_DIR, deps, false).catch(() => {})
  const host = (window as Window & { __dclEditorHost?: DclEditorHostV1 }).__dclEditorHost!

  host.setSceneInset({ left: 300.4, top: 52, right: 340, bottom: 0 })
  host.setSceneInset({ left: -1, top: 52, right: 340, bottom: 0 })
  host.setSceneInset({ left: Number.NaN, top: 52, right: 340, bottom: 0 })
  expect(setInteractableArea.mock.calls).toEqual([[{ left: 300, top: 52, right: 340, bottom: 0 }]])
  vi.unstubAllGlobals()
})
