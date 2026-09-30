import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { FriendAction } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: friends — friends/requests/blocked snapshot + every social action.
describe('friends domain', () => {
  const ops: FriendAction[] = ['request', 'accept', 'reject', 'cancel', 'delete', 'block', 'unblock']

  it.each(ops)('act(%s) posts the matching friendAction', async (op) => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().friends.act(op, '0xdead'))
    expect(h.driver.last('friendAction')).toEqual({ kind: 'friendAction', op, address: '0xdead' })
  })

  it('friends stream updates list / requests / blocked / availability', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({
      kind: 'friends',
      available: true,
      friends: [{ address: '0x1', name: 'A', status: 'online' }],
      received: [{ address: '0x2', name: 'B', id: 'r1' }],
      sent: [{ address: '0x3', name: 'C', id: 's1' }],
      blocked: ['0x4']
    })
    const f = h.session().friends
    expect(f.available).toBe(true)
    expect(f.list).toHaveLength(1)
    expect(f.received[0].id).toBe('r1')
    expect(f.sent[0].id).toBe('s1')
    expect(f.blocked).toEqual(['0x4'])
  })
})
