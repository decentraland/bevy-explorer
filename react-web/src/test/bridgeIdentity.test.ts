import { describe, it, expect, vi } from 'vitest'
import { createIdentityHub } from '../../bridge-scene/src/identity'

describe('bridge identity hub', () => {
  it('resets per-account state only when a different account appears', () => {
    const hub = createIdentityHub()
    const reset = vi.fn()
    hub.onChange(reset)
    hub.observe(null)
    hub.observe('0xa')
    hub.observe('0xa')
    expect(reset).not.toHaveBeenCalled()
    hub.observe(null) // logged out: nothing to reset yet
    hub.observe('0xb')
    expect(reset).toHaveBeenCalledTimes(1)
    hub.observe('0xb')
    expect(reset).toHaveBeenCalledTimes(1)
  })

  it('one failing reset does not stop the others', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    const hub = createIdentityHub()
    const after = vi.fn()
    hub.onChange(() => {
      throw new Error('boom')
    })
    hub.onChange(after)
    hub.observe('0xa')
    hub.observe('0xb')
    expect(after).toHaveBeenCalledTimes(1)
  })
})
