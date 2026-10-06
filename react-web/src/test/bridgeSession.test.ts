import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { Ctx } from '../../bridge-scene/src/bridge'
import type { PageToScene, SceneToPage } from '../engine/protocol'

// A frame loop shaped like @dcl/ecs: systems run from the live array, which removal splices.
const systems: Array<(dt: number) => void> = []
vi.mock('../../bridge-scene/node_modules/@dcl/sdk/ecs.js', () => ({
  engine: {
    addSystem: (fn: (dt: number) => void) => systems.push(fn),
    removeSystem: (fn: (dt: number) => void) => {
      const i = systems.indexOf(fn)
      if (i >= 0) systems.splice(i, 1)
    }
  }
}))
let player: { userId: string } | null = null
vi.mock('../../bridge-scene/node_modules/@dcl/sdk/players/index.js', () => ({ getPlayer: () => player }))
vi.mock('../../bridge-scene/src/bevy-api', () => ({
  BevyApi: {
    loginPrevious: vi.fn(async () => ({ success: true, error: '' })),
    logout: vi.fn(),
    getSceneLoadingUIStream: vi.fn(() => new Promise(() => {}))
  }
}))

const { registerSession } = await import('../../bridge-scene/src/domains/session')
const { waitMs } = await import('../../bridge-scene/src/system-helpers')

function fakeCtx(): { ctx: Ctx; sent: SceneToPage[]; req: (msg: PageToScene) => Promise<void>; frame: () => void } {
  const sent: SceneToPage[] = []
  const handlers = new Map<string, Array<(m: PageToScene) => unknown>>()
  const pushed: Array<(dt: number) => void> = []
  const ctx: Ctx = {
    send: (m) => sent.push(m),
    on: (kind, h) => handlers.set(kind, [...(handlers.get(kind) ?? []), h as (m: PageToScene) => unknown]),
    push: (s) => pushed.push(s)
  }
  const req = async (msg: PageToScene): Promise<void> => {
    await Promise.all((handlers.get(msg.kind) ?? []).map((h) => h(msg)))
  }
  return { ctx, sent, req, frame: () => pushed.forEach((s) => s(1 / 60)) }
}

const readies = (sent: SceneToPage[]): number => sent.filter((m) => m.kind === 'event' && m.name === 'playerReady').length

beforeEach(() => {
  systems.length = 0
  player = null
})

describe('bridge session', () => {
  it('logging back into the same account announces playerReady again', async () => {
    const { ctx, sent, req, frame } = fakeCtx()
    registerSession(ctx)
    player = { userId: '0xa' }
    frame()
    expect(readies(sent)).toBe(1)
    await req({ kind: 'rpc:req', id: '1', method: 'logout' })
    frame() // no frame has seen the player gone yet
    expect(readies(sent)).toBe(1)
    await req({ kind: 'rpc:req', id: '2', method: 'loginPrevious' })
    frame()
    expect(readies(sent)).toBe(2)
  })

  it('announces playerReady again once the engine has cleared the player and set the next one', () => {
    const { ctx, sent, frame } = fakeCtx()
    registerSession(ctx)
    player = { userId: '0xa' }
    frame()
    player = null // logout
    frame()
    expect(readies(sent)).toBe(1)
    player = { userId: '0xb' } // a login the bridge did not relay (the web console command)
    frame()
    expect(readies(sent)).toBe(2)
  })
})

describe('waitMs', () => {
  it('does not make other systems skip a frame when it finishes', async () => {
    const other = vi.fn()
    const done = waitMs(10)
    systems.push(other)
    for (const s of systems) s(1)
    await done
    expect(other).toHaveBeenCalledTimes(1)
  })
})
