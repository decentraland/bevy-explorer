// The scene editor host (features/editorHost): the page offers Create only where an editor is
// available, loads it only when the Create page opens, and what it hands the editor keeps the
// engine on a short leash. What it signs for the editor is in editorHostSigning.test.tsx.
import { describe, it, expect, vi } from 'vitest'
import { act, render, renderHook, screen } from '@testing-library/react'
import { ToastHost } from '../design'
import userEvent from '@testing-library/user-event'
import { editorEntry, editorSource } from '../features/editorHost/config'
import { CreatePage } from '../features/editorHost/CreatePage'
import { EditorEntryContext } from '../features/editorHost/entry'
import type { EditorHostScript } from '../features/editorHost/host/editorHost'
import { guardUrlSync, loadEditor, type DclEditorHostV1, type EditorPackage } from '../features/editorHost/host/host'
import { useEditorHost } from '../features/editorHost/useEditorHost'
import type { EngineSession } from '../features/session/useEngineSession'
import { MainMenuShell } from '../features/menu/MainMenuShell'
import { Sidebar } from '../features/sidebar/Sidebar'
import { PAGE_DIR } from '../lib/publicUrl'
import { fakeSession } from './harness'

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  __dclEditorHostScript?: (script: EditorHostScript) => void
  __dclEditor?: Partial<EditorPackage>
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

  it('offers Create in the rail and the menu top bar only where an editor is available; both open the Create page', async () => {
    const navigate = vi.fn()
    const s = fakeSession()
    const surfaces = (
      <>
        <Sidebar session={s} />
        <MainMenuShell active="settings" onNavigate={navigate} onClose={vi.fn()}>
          <div />
        </MainMenuShell>
      </>
    )
    const plain = render(surfaces)
    expect(screen.queryAllByRole('button', { name: /Create/ })).toEqual([])
    plain.unmount()

    render(<EditorEntryContext.Provider value={{ load: vi.fn(), loading: false }}>{surfaces}</EditorEntryContext.Provider>)
    const [rail, topBar] = screen.getAllByRole('button', { name: /Create/ })
    await userEvent.click(rail)
    expect(s.create.toggle).toHaveBeenCalledTimes(1)
    await userEvent.click(topBar)
    expect(navigate).toHaveBeenCalledWith('create')
  })

  it('loads nothing until the Create page opens; mounts the home in its body, and keeps the engine to preview realms', async () => {
    const synced = vi.fn()
    w.set_url_params = synced
    const engineConsole = vi.fn(async (line: string) => (line === '/time' ? 'time 10:30 -> 10:30, speed 7 (elapsed: 37800)' : ''))
    w.engine_console_command = engineConsole
    history.replaceState(null, '', '/?realm=boedo.dcl.eth&position=3,4')
    const travel = vi.fn(async () => {})
    session.editor.travel = travel
    const source = editorSource('', 'localhost', PAGE_DIR)

    function Page({ s }: { s: EngineSession }): React.JSX.Element {
      return (
        <EditorEntryContext.Provider value={useEditorHost(source, s)}>
          <CreatePage create={s.create} profile={s.profile} onNavigate={vi.fn()} />
          <ToastHost />
        </EditorEntryContext.Provider>
      )
    }
    const view = render(<Page s={session} />)
    expect(scripts()).toEqual([])
    expect(w.__dclEditorHost).toBeUndefined()

    const opened = { ...session, create: { ...session.create, open: true } }
    view.rerender(<Page s={opened} />)
    expect(screen.getByText('Opening Create…')).toBeInTheDocument()
    expect(scripts()).toEqual([expect.stringContaining('editorHost')])
    await act(async () => w.__dclEditorHostScript!({ guardUrlSync, loadEditor }))
    expect(host()).toMatchObject({
      version: 1,
      pageDir: PAGE_DIR,
      editorBase: `${PAGE_DIR}editor/`,
      openProject: null,
      services: { projects: 'http://localhost:8787', worldsContent: 'https://worlds-content-server.decentraland.org' }
    })
    expect(host().container.parentElement).toBe(document.body)
    expect(host().container.style.pointerEvents).toBe('none')

    // the package fails to load: a toast, and the page closes
    const first = document.querySelector<HTMLScriptElement>('script[src$="editor.js"]')!
    expect(first.src).toBe(`${PAGE_DIR}editor/editor.js`)
    await act(async () => first.dispatchEvent(new Event('error')))
    expect(await screen.findByText(/Create could not be loaded/)).toBeInTheDocument()
    expect(session.create.show).toHaveBeenLastCalledWith(false)

    view.rerender(<Page s={session} />)
    await act(async () => view.rerender(<Page s={opened} />))
    const retry = document.querySelector<HTMLScriptElement>('script[src*="editor.js?retry"]')!
    const unmountHome = vi.fn()
    const editor = { mountHome: vi.fn((_el: HTMLElement, _api: { close: () => void }) => unmountHome), unmount: vi.fn() }
    w.__dclEditor = editor
    await act(async () => retry.dispatchEvent(new Event('load')))
    expect(editor.mountHome).toHaveBeenCalledTimes(1)
    expect(editor.mountHome.mock.lastCall![0]).toBe(document.getElementById('dcl-editor-home'))
    expect(screen.queryByText('Opening Create…')).toBeNull()

    // the editor opens a scene: it closes the page, and the home unmounts
    act(() => editor.mountHome.mock.lastCall![1].close())
    expect(session.create.show).toHaveBeenLastCalledWith(false)
    view.rerender(<Page s={session} />)
    expect(unmountHome).toHaveBeenCalledTimes(1)

    await expect(host().openPreview('https://evil.example', '0,0')).rejects.toThrow()
    expect(travel).not.toHaveBeenCalled()
    const realm = `${PAGE_DIR}preview/my-scene`
    await host().openPreview('my-scene', '4,-2')
    expect(travel).toHaveBeenLastCalledWith(realm, { x: 4, y: -2 })

    // the engine's url sync, as boot.js receives it while on the preview realm
    w.set_url_params!(JSON.stringify({ realm, position: '4,-2', editor: false }))
    expect(JSON.parse(synced.mock.lastCall![0] as string)).toEqual({ realm: 'boedo.dcl.eth', position: '3,4', editor: false })

    // back to scenes: the editor's scene is left as on exit, and the Create page opens
    let landed = (): void => {}
    travel.mockImplementationOnce(() => new Promise<void>((resolve) => (landed = resolve)))
    host().openCreatePage()
    expect(editor.unmount).toHaveBeenCalledTimes(1)
    expect(session.editor.setMode).toHaveBeenLastCalledWith('off')
    expect(travel).toHaveBeenLastCalledWith('boedo.dcl.eth', { x: 3, y: 4 })
    expect(session.create.show).toHaveBeenLastCalledWith(true)
    // the inspection pin is cleared and the clock is the one read on the way in
    await vi.waitFor(() => expect(engineConsole.mock.calls.map(([line]) => line)).toEqual(['/time', '/set_scene', '/time 10.5 7']))
    // a scene opened again before the travel back lands still goes home to where the player was
    w.set_url_params!(JSON.stringify({ realm, position: '4,-2', editor: false }))
    expect(JSON.parse(synced.mock.lastCall![0] as string)).toMatchObject({ realm: 'boedo.dcl.eth', position: '3,4' })
    await host().openPreview('my-scene', '4,-2')
    await act(async () => landed())
    host().openCreatePage()
    expect(travel).toHaveBeenLastCalledWith('boedo.dcl.eth', { x: 3, y: 4 })

    host().exit()
    expect(session.create.show).toHaveBeenLastCalledWith(false)
    expect(scripts().filter((src) => src.includes('editor.js'))).toHaveLength(1)
  })
})
