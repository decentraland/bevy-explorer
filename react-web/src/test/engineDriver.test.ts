import { describe, it, expect, vi } from 'vitest'
import { EngineDriver } from '../engine/EngineDriver'
import { EngineRpc } from '../engine/engineRpc'
import { sessionId } from '../lib/analytics'

describe('EngineDriver', () => {
  it('launches with the page analytics session id beside the host options', () => {
    const rpc = new EngineRpc()
    const launch = vi.spyOn(rpc, 'launch').mockImplementation(() => {})
    const driver = new EngineDriver(rpc)
    driver.launch('realm', '1,2', { holdWorld: true })
    driver.launch('realm')
    expect(launch).toHaveBeenNthCalledWith(1, 'realm', '1,2', { holdWorld: true, sessionId: sessionId() })
    expect(launch).toHaveBeenNthCalledWith(2, 'realm', undefined, { sessionId: sessionId() })
    expect(sessionId()).toMatch(/^[0-9a-f-]{36}$/)
    driver.dispose()
  })
})
