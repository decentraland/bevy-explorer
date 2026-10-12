import { describe, it, expect, vi, afterEach } from 'vitest'
import { EngineDriver } from '../engine/EngineDriver'
import { EngineRpc } from '../engine/engineRpc'
import type { SceneToPage } from '../engine/protocol'

afterEach(() => vi.useRealTimers())

describe('EngineDriver', () => {
  it('hands off to the world again for an account logged in after a logout', async () => {
    vi.useFakeTimers()
    const rpc = new EngineRpc()
    vi.spyOn(rpc, 'command').mockResolvedValue('')
    const driver = new EngineDriver(rpc)
    const ready: SceneToPage[] = []
    driver.on((m) => { if (m.kind === 'event' && m.name === 'playerReady') ready.push(m) })
    await driver.loginGuest()
    await vi.advanceTimersByTimeAsync(6000)
    expect(ready).toHaveLength(1)
    await driver.logout()
    await driver.loginGuest()
    await vi.advanceTimersByTimeAsync(6000)
    expect(ready).toHaveLength(2)
    driver.dispose()
  })
})
