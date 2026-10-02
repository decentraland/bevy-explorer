// The scene editor host (features/editorHost): the page loads the editor package only for
// `?editor` on an allowed host, and what it hands the editor keeps the engine on a short leash.
import { describe, it, expect, vi } from 'vitest'
import { renderHook } from '@testing-library/react'
import { editorSource } from '../features/editorHost/config'
import type { DclEditorHostV0 } from '../features/editorHost/host'
import { useEditorHost } from '../features/editorHost/useEditorHost'
import { PAGE_DIR } from '../lib/publicUrl'
import { fakeSession } from './harness'

type HostWindow = Window & { __dclEditorHost?: DclEditorHostV0; set_url_params?: (json: string) => void }
const w = window as HostWindow

const loaded = (): unknown[] =>
  [w.__dclEditorHost, document.getElementById('dcl-editor-host'), document.querySelector('script[src$="editor.js"]')].filter(
    (x) => x != null
  )

describe('editor host', () => {
  it.each([
    ['no editor param', '?realm=x', 'localhost'],
    ['an untrusted host', '?editor', 'evil.example'],
    ['a lookalike of an allowed host', '?editor', 'decentraland.zone.evil.example']
  ])('loads nothing with %s', (_, search, hostname) => {
    renderHook(() => useEditorHost(editorSource(search, hostname, PAGE_DIR), fakeSession()))
    expect(loaded()).toEqual([])
  })

  it('loads the package on localhost, travels only to preview realms and keeps them out of the url', async () => {
    const synced = vi.fn()
    w.set_url_params = synced
    history.replaceState(null, '', '/?editor&realm=boedo.dcl.eth&position=3,4')
    const session = fakeSession()
    const travel = vi.fn(async () => {})
    session.editor.travel = travel

    renderHook(() => useEditorHost(editorSource('?editor', 'localhost', PAGE_DIR), session))
    const host = w.__dclEditorHost!
    expect(host).toMatchObject({ version: 0, pageDir: PAGE_DIR, editorBase: `${PAGE_DIR}editor/` })
    expect(host.container.parentElement).toBe(document.body)
    expect(host.container.style.pointerEvents).toBe('none')
    expect(document.querySelector<HTMLScriptElement>('script[type="module"]')?.src).toBe(`${PAGE_DIR}editor/editor.js`)

    await expect(host.openPreview('https://evil.example', '0,0')).rejects.toThrow()
    expect(travel).not.toHaveBeenCalled()

    const realm = `${PAGE_DIR}preview/my-scene`
    await host.openPreview('my-scene', '4,-2')
    expect(travel).toHaveBeenLastCalledWith(realm, { x: 4, y: -2 })

    // the engine's url sync, as boot.js receives it while on the preview realm
    w.set_url_params!(JSON.stringify({ realm, position: '4,-2', editor: false }))
    expect(JSON.parse(synced.mock.lastCall![0] as string)).toEqual({ realm: 'boedo.dcl.eth', position: '3,4', editor: true })

    host.exit()
    expect(session.editor.setMode).toHaveBeenLastCalledWith('off')
    expect(travel).toHaveBeenLastCalledWith('boedo.dcl.eth', { x: 3, y: 4 })
  })
})
