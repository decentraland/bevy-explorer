// The scene editor host (features/editorHost): the page offers Create only where an editor is
// available, loads it only when asked, and what it hands the editor keeps the engine on a short
// leash. What it signs for the editor is in editorHostSigning.test.tsx.
import { describe, it, expect, vi } from 'vitest'
import { act, render, renderHook, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { editorEntry, editorSource } from '../features/editorHost/config'
import { EditorEntryContext } from '../features/editorHost/entry'
import type { EditorHostScript } from '../features/editorHost/host/editorHost'
import { guardUrlSync, openEditor, type DclEditorHostV1 } from '../features/editorHost/host/host'
import { useEditorHost } from '../features/editorHost/useEditorHost'
import { MainMenuShell } from '../features/menu/MainMenuShell'
import { Sidebar } from '../features/sidebar/Sidebar'
import { PAGE_DIR } from '../lib/publicUrl'
import { fakeSession } from './harness'

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  __dclEditorHostScript?: (script: EditorHostScript) => void
  __dclEditor?: { mount: () => void; unmount: () => void }
  set_url_params?: (json: string) => void
  engine_console_command?: (line: string) => Promise<string>
}
const w = window as HostWindow

const scripts = (): string[] => [...document.querySelectorAll('script')].map((s) => s.src)
const session = fakeSession()
const host = (): DclEditorHostV1 => w.__dclEditorHost!

describe('editor host', () => {
  it.each([
    ['an untrusted host', 'evil.example'],
    ['a lookalike of an allowed host', 'decentraland.zone.evil.example']
  ])('has no editor on %s, even with ?editor', (_, hostname) => {
    history.replaceState(null, '', '/?editor')
    const { result } = renderHook(() => useEditorHost(editorSource('?editor', hostname, PAGE_DIR), fakeSession()))
    expect(result.current).toBeNull()
    expect(scripts()).toEqual([])
  })

  it('reads the project and the local service overrides from the entry url', () => {
    expect(editorEntry('?realm=x')).toEqual({ open: false, project: null })
    expect(editorEntry('?editor')).toEqual({ open: true, project: null })
    // what the engine's url sync turns a bare flag into
    expect(editorEntry('?editor=true')).toEqual({ open: true, project: null })
    expect(editorEntry('?editor=my-scene')).toEqual({ open: true, project: 'my-scene' })
    expect(editorEntry('?editor=../x')).toEqual({ open: true, project: null })
    expect(editorSource('?editor-projects=http://localhost:9000/&editor-worlds=javascript:alert(1)', 'localhost', PAGE_DIR)?.services).toEqual({
      projects: 'http://localhost:9000',
      worldsContent: 'https://worlds-content-server.decentraland.org'
    })
  })

  it('offers Create in the rail and the menu top bar only where an editor is available; the top bar closes its page', async () => {
    const closeMenu = vi.fn()
    const surfaces = (
      <>
        <Sidebar session={fakeSession()} />
        <MainMenuShell active="settings" onNavigate={vi.fn()} onClose={closeMenu}>
          <div />
        </MainMenuShell>
      </>
    )
    const plain = render(surfaces)
    expect(screen.queryAllByRole('button', { name: /Create/ })).toEqual([])
    plain.unmount()

    const open = vi.fn()
    render(<EditorEntryContext.Provider value={{ open, loading: false }}>{surfaces}</EditorEntryContext.Provider>)
    const [rail, topBar] = screen.getAllByRole('button', { name: /Create/ })
    await userEvent.click(rail)
    expect(open).toHaveBeenCalledTimes(1)
    expect(closeMenu).not.toHaveBeenCalled()
    await userEvent.click(topBar)
    expect(open).toHaveBeenCalledTimes(2)
    expect(closeMenu).toHaveBeenCalledTimes(1)
  })

  it('loads nothing until Create is opened; then keeps the engine to preview realms, and comes back in after an exit', async () => {
    const synced = vi.fn()
    w.set_url_params = synced
    const engineConsole = vi.fn(async (line: string) => (line === '/time' ? 'time 10:30 -> 10:30, speed 7 (elapsed: 37800)' : ''))
    w.engine_console_command = engineConsole
    history.replaceState(null, '', '/?realm=boedo.dcl.eth&position=3,4')
    const travel = vi.fn(async () => {})
    session.editor.travel = travel

    const { result } = renderHook(() => useEditorHost(editorSource('', 'localhost', PAGE_DIR), session))
    expect(scripts()).toEqual([])
    expect(w.__dclEditorHost).toBeUndefined()

    act(() => result.current!.open())
    expect(result.current!.loading).toBe(true)
    expect(scripts()).toEqual([expect.stringContaining('editorHost')])
    // the host script runs
    await act(async () => w.__dclEditorHostScript!({ guardUrlSync, openEditor }))

    expect(host()).toMatchObject({
      version: 1,
      pageDir: PAGE_DIR,
      editorBase: `${PAGE_DIR}editor/`,
      openProject: null,
      services: { projects: 'http://localhost:8787', worldsContent: 'https://worlds-content-server.decentraland.org' }
    })
    expect(host().container.parentElement).toBe(document.body)
    expect(host().container.style.pointerEvents).toBe('none')
    const editorScript = document.querySelector<HTMLScriptElement>('script[src$="editor.js"]')!
    expect(editorScript.src).toBe(`${PAGE_DIR}editor/editor.js`)
    // the package runs
    const editor = { mount: vi.fn(), unmount: vi.fn() }
    w.__dclEditor = editor
    await act(async () => editorScript.dispatchEvent(new Event('load')))
    expect(result.current!.loading).toBe(false)

    await expect(host().openPreview('https://evil.example', '0,0')).rejects.toThrow()
    expect(travel).not.toHaveBeenCalled()

    const realm = `${PAGE_DIR}preview/my-scene`
    await host().openPreview('my-scene', '4,-2')
    expect(travel).toHaveBeenLastCalledWith(realm, { x: 4, y: -2 })

    // the engine's url sync, as boot.js receives it while on the preview realm
    w.set_url_params!(JSON.stringify({ realm, position: '4,-2', editor: false }))
    expect(JSON.parse(synced.mock.lastCall![0] as string)).toEqual({ realm: 'boedo.dcl.eth', position: '3,4', editor: false })

    host().exit()
    expect(editor.unmount).toHaveBeenCalledTimes(1)
    expect(session.editor.setMode).toHaveBeenLastCalledWith('off')
    expect(travel).toHaveBeenLastCalledWith('boedo.dcl.eth', { x: 3, y: 4 })
    // the inspection pin is cleared and the clock is the one read on the way in
    await vi.waitFor(() => expect(engineConsole.mock.calls.map(([line]) => line)).toEqual(['/time', '/set_scene', '/time 10.5 7']))

    await act(async () => result.current!.open())
    expect(editor.mount).toHaveBeenCalledTimes(1)
    expect(scripts().filter((src) => src.includes('editor.js'))).toHaveLength(1)
  })
})
