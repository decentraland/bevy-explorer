// The scene editor host (features/editorHost): the page offers Create only where an editor is
// available, loads it only when the Create page opens, and what it hands the editor keeps the
// engine on a short leash. What it signs for the editor is in editorHostSigning.test.tsx.
import { describe, it, expect, vi } from 'vitest'
import { act, render, renderHook, screen } from '@testing-library/react'
import { ToastHost } from '../design'
import userEvent from '@testing-library/user-event'
import { editorOffered, EditorOffered } from '../features/editorHost/config'
import { CreatePage } from '../features/editorHost/CreatePage'
import EditorHost, { useEditorHost } from '../features/editorHost/EditorHost'
import { type DclEditorHostV1, type EditorPackage } from '../features/editorHost/host/host'
import { editorEntry, editorSource } from '../features/editorHost/source'
import type { EngineSession } from '../features/session/useEngineSession'
import { MainMenuShell } from '../features/menu/MainMenuShell'
import { Sidebar } from '../features/sidebar/Sidebar'
import { bridgeChannelName, type Envelope } from '../engine/protocol'
import { PAGE_DIR } from '../lib/publicUrl'
import { fakeSession } from './harness'

type HostWindow = Window & {
  __dclEditorHost?: DclEditorHostV1
  __dclEditor?: Partial<EditorPackage>
  set_url_params?: (json: string) => void
  engine_console_command?: (line: string) => Promise<string>
  __bevyStartServer?: (options: unknown) => Promise<unknown>
}
const w = window as HostWindow

const scripts = (): string[] => [...document.querySelectorAll('script')].map((s) => s.src)
const session = fakeSession()
const host = (): DclEditorHostV1 => w.__dclEditorHost!

describe('editor host', () => {
  it.each([
    ['an untrusted host', 'evil.example'],
    ['a lookalike of an allowed host', 'decentraland.zone.evil.example']
  ])('has no editor on %s', (_, hostname) => {
    expect(editorOffered(hostname)).toBe(false)
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
      worldsContent: 'https://worlds-content-server.decentraland.org',
      signed: {
        worldsContent: 'https://worlds-content-server.decentraland.org',
        commsGatekeeper: 'https://comms-gatekeeper.decentraland.org',
        storage: 'https://storage.decentraland.org',
        creatorsData: 'https://creators-data.decentraland.org/v2',
        multiplayer: 'https://multiplayer-server.decentraland.org'
      }
    })
    // a local Worlds server: nothing else is signed for
    expect(editorSource('?editor-projects=https://evil.example/v1&editor-worlds=http://127.0.0.1:8799', 'localhost', PAGE_DIR)?.services).toEqual({
      projects: 'http://localhost:8787',
      worldsContent: 'http://127.0.0.1:8799'
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

    render(<EditorOffered.Provider value>{surfaces}</EditorOffered.Provider>)
    const [rail, topBar] = screen.getAllByRole('button', { name: /Create/ })
    await userEvent.click(rail)
    expect(s.create.toggle).toHaveBeenCalledTimes(1)
    await userEvent.click(topBar)
    expect(navigate).toHaveBeenCalledWith('create')
  })

  it('loads nothing until the Create page opens; mounts the home in its body, and keeps the engine to preview realms', async () => {
    const synced = vi.fn()
    w.set_url_params = synced
    // the preview realm: no scene server for this scene
    vi.stubGlobal('fetch', async () => new Response(null, { status: 404 }))
    const engineConsole = vi.fn(async (line: string) => (line === '/time' ? 'time 10:30 -> 10:30, speed 7 (elapsed: 37800)' : ''))
    w.engine_console_command = engineConsole
    history.replaceState(null, '', '/?realm=boedo.dcl.eth&position=3,4')
    // the bridge scene: the host's travels reach it, and it answers each
    const travel = vi.fn()
    let held: (() => void) | null = null
    let holdNext = false
    const bridge = new BroadcastChannel(bridgeChannelName())
    bridge.onmessage = ({ data }: MessageEvent<Envelope>) => {
      if (data.to !== 'scene' || (data.msg.kind !== 'teleport' && data.msg.kind !== 'changeRealm')) return
      const msg = data.msg
      travel(msg.realm, msg.kind === 'teleport' ? { x: msg.x, y: msg.y } : undefined)
      const answer = (): void => bridge.postMessage({ to: 'page', msg: { kind: 'travelResult', travelId: msg.travelId!, realm: msg.realm!, ok: true } } satisfies Envelope)
      if (holdNext) [held, holdNext] = [answer, false]
      else answer()
    }

    function Page({ s }: { s: EngineSession }): React.JSX.Element {
      return (
        <>
          <CreatePage entry={useEditorHost('', s)!} create={s.create} profile={s.profile} onNavigate={vi.fn()} />
          <ToastHost />
        </>
      )
    }
    const view = render(<Page s={session} />)
    expect(scripts()).toEqual([])
    expect(w.__dclEditorHost).toBeUndefined()

    const opened = { ...session, create: { ...session.create, open: true } }
    view.rerender(<Page s={opened} />)
    expect(screen.getByText('Opening Create…')).toBeInTheDocument()
    await act(async () => {})
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

    // the console takes the editor's own commands, about the scene it edits, and nothing else
    const own = `b64-${btoa('/preview/my-scene-m1')}`
    for (const line of ['/spawn https://evil.example/x true', '/kill x', '/logout', '/login_identity x', '/changerealm x', `/reload b64-${btoa('/preview/other-m1')}`, '/reload', '/set_scene bafkreihud'])
      await expect(host().engineConsole(line), line).rejects.toThrow('not-allowed')
    await host().engineConsole(`/reload ${own}`)
    await host().engineConsole(`set_scene ${own}`)
    expect(engineConsole.mock.calls.map(([line]) => line)).toEqual(['/time', `/reload ${own}`, `set_scene ${own}`])
    engineConsole.mockClear()

    // the engine's url sync, as boot.js receives it while on the preview realm
    w.set_url_params!(JSON.stringify({ realm, position: '4,-2', editor: false }))
    expect(JSON.parse(synced.mock.lastCall![0] as string)).toEqual({ realm: 'boedo.dcl.eth', position: '3,4', editor: false })

    // back to scenes: the editor's scene is left as on exit, and the Create page opens
    holdNext = true
    host().openCreatePage()
    expect(editor.unmount).toHaveBeenCalledTimes(1)
    expect(session.editor.setMode).toHaveBeenLastCalledWith('off')
    await vi.waitFor(() => expect(travel).toHaveBeenLastCalledWith('boedo.dcl.eth', { x: 3, y: 4 }))
    expect(session.create.show).toHaveBeenLastCalledWith(true)
    // the inspection pin is cleared and the clock is the one read on the way in
    await vi.waitFor(() => expect(engineConsole.mock.calls.map(([line]) => line)).toEqual(['/set_scene', '/time 10.5 7']))
    // a scene opened again before the travel back lands still goes home to where the player was
    w.set_url_params!(JSON.stringify({ realm, position: '4,-2', editor: false }))
    expect(JSON.parse(synced.mock.lastCall![0] as string)).toMatchObject({ realm: 'boedo.dcl.eth', position: '3,4' })
    await host().openPreview('my-scene', '4,-2')
    await act(async () => held!())
    host().openCreatePage()
    await vi.waitFor(() => expect(travel).toHaveBeenLastCalledWith('boedo.dcl.eth', { x: 3, y: 4 }))

    host().exit()
    bridge.close()
    expect(session.create.show).toHaveBeenLastCalledWith(false)
    expect(scripts().filter((src) => src.includes('editor.js'))).toHaveLength(1)
    vi.unstubAllGlobals()
  })

  it("runs an authoritative preview's scene server beside the client, one at a time, until the preview is left", async () => {
    const { result } = renderHook(() => useEditorHost('', session))
    await act(async () => void result.current!.load().catch(() => {}))
    const bridge = new BroadcastChannel(bridgeChannelName())
    bridge.onmessage = ({ data }: MessageEvent<Envelope>) => {
      const msg = data.msg
      if (data.to === 'scene' && (msg.kind === 'teleport' || msg.kind === 'changeRealm'))
        bridge.postMessage({ to: 'page', msg: { kind: 'travelResult', travelId: msg.travelId!, realm: msg.realm!, ok: true } } satisfies Envelope)
    }
    // the preview realm's scene.json, as the service worker serves it
    vi.stubGlobal('fetch', async (url: string) =>
      url.endsWith('/scene.json') ? Response.json({ main: 'bin/index.js', authoritativeMultiplayer: url.includes('/preview/game/') }) : new Response(null, { status: 404 })
    )
    // engine.js: each server is a hidden frame of the page
    const startServer = vi.fn(async (_options: unknown) => {
      const frame = document.createElement('iframe')
      frame.src = `${PAGE_DIR}engine/headless.html`
      document.body.appendChild(frame)
      return {}
    })
    w.__bevyStartServer = startServer
    const servers = (): number => document.querySelectorAll('iframe[src$="/headless.html"]').length
    const realm = `${PAGE_DIR}preview/game`

    await host().openPreview('game', '4,-2')
    expect(startServer.mock.calls).toEqual([[{ realm, position: '4,-2', preview: true }]])
    expect(servers()).toBe(1)
    // opened again: the old server goes, a new one serves it
    await host().openPreview('game', '5,-2')
    expect(startServer.mock.calls).toEqual([[{ realm, position: '4,-2', preview: true }], [{ realm, position: '5,-2', preview: true }]])
    expect(servers()).toBe(1)
    // a scene with no server of its own
    await host().openPreview('plain', '0,0')
    expect(startServer).toHaveBeenCalledTimes(2)
    expect(servers()).toBe(0)

    await host().openPreview('game', '4,-2')
    expect(servers()).toBe(1)
    host().exit()
    await vi.waitFor(() => expect(servers()).toBe(0))
    expect(startServer).toHaveBeenCalledTimes(3)
    bridge.close()
    delete w.__bevyStartServer
    vi.unstubAllGlobals()
  })

  it('keeps the Create page over the travel back to scenes', () => {
    const travelling: EngineSession = { ...session, phase: 'entering', create: { ...session.create, open: true } }
    render(<EditorHost session={travelling} entrySearch="" onNavigate={vi.fn()} />)
    expect(document.getElementById('dcl-editor-home')).toBeInTheDocument()
  })
})
