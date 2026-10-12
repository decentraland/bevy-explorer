import { describe, it, expect, vi } from 'vitest'
import type { PageToScene, SceneToPage } from '../engine/protocol'

const consoleCommand = vi.fn(async () => '')
vi.mock('../../bridge-scene/src/bevy-api', () => ({
  BevyApi: {
    consoleCommand,
    setPermanentPermission: vi.fn(),
    liveSceneInfo: async () => [{ hash: HASH, isSuper: true, isBroken: false }]
  }
}))
vi.mock('../../bridge-scene/src/system-helpers', () => ({ waitMs: async () => undefined }))

const HASH = 'bafkreigergyqmm6l6uqonpgiumbswhd7cskutmfrntyv2vx4ffeqz3fyya'
const PAGE = 'https://play.example/bevy-web/'

describe('bridge scene: the editor domain', () => {
  it('spawns the editor scene only from the realm of the page that said hello first', async () => {
    const { registerEditor } = await import('../../bridge-scene/src/domains/editor')
    const handlers = new Map<string, (msg: PageToScene) => Promise<void> | void>()
    const sent: SceneToPage[] = []
    registerEditor({ send: (msg) => sent.push(msg), on: (kind, handler) => handlers.set(kind, handler as never), push: vi.fn() })
    const hello = (pageDir: string): unknown => handlers.get('hello')!({ kind: 'hello', pageDir })
    const spawn = (id: number, source: string): Promise<void> | void => handlers.get('editorScene')!({ kind: 'editorScene', id, action: 'spawn', source, hash: HASH })

    await hello(PAGE)
    // a privileged scene on the same channel can say hello too, later
    await hello('https://attacker.example/')
    await spawn(1, `https://attacker.example/editor-scene/${HASH}`)
    await spawn(2, `${PAGE}editor-scene/${HASH}`)

    expect(sent).toEqual([
      { kind: 'editorSceneResult', id: 1, ok: false, error: 'not the editor scene' },
      { kind: 'editorSceneResult', id: 2, ok: true }
    ])
    expect(consoleCommand.mock.calls).toEqual([['spawn', [`${PAGE}editor-scene/${HASH}`, 'true']]])
  })
})
