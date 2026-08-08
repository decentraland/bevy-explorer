import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import { renderSession, enterAsGuest } from './harness'
import { DEFAULT_REALM } from '../features/engine/EngineHost'

// DOMAIN: chat — send Nearby messages, receive relayed chat, nearby members roster.
describe('chat domain', () => {
  it('send posts a Nearby sendChat message', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.send('hello world'))
    expect(h.driver.last('sendChat')).toEqual({
      kind: 'sendChat',
      message: 'hello world',
      channel: 'Nearby'
    })
  })

  it('send trims and ignores blank messages', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.send('   '))
    expect(h.driver.sentOf('sendChat')).toHaveLength(0)
    act(() => h.session().chat.send('  hi  '))
    expect(h.driver.last('sendChat')?.message).toBe('hi')
  })

  it('relayed chat messages append to the log', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: '0xabc', message: 'gm', channel: 'Nearby' } })
    const msgs = h.session().chat.messages
    expect(msgs).toHaveLength(1)
    expect(msgs[0]).toMatchObject({ sender: '0xabc', message: 'gm', channel: 'Nearby' })
  })

  // `/goto main` must name a destination, not just a realm: a bare realm change leaves the engine
  // to keep the parcel you were standing on (clamped into Genesis' bounds), which from a World
  // drops you on an arbitrary — often empty — Genesis parcel. The destination is the plaza's BASE
  // parcel (0,0); #1042 fixed the engine-side bug that let a stale World-realm pointer resolve
  // there and skip the loading screen.
  it('/goto main teleports to Genesis Plaza, not just to the Genesis realm', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.send('/goto main'))
    expect(h.driver.last('teleport')).toEqual({ kind: 'teleport', realm: DEFAULT_REALM, x: 0, y: 0 })
    expect(h.driver.sentOf('changeRealm')).toHaveLength(0)
    expect(h.driver.sentOf('sendChat')).toHaveLength(0) // never broadcast as a chat line
  })

  // Testing aid (backlog 45): an explicit x,y after the destination overrides the default parcel,
  // so a specific Genesis parcel or world parcel can be targeted directly from chat.
  it('/goto genesis x,y overrides the default parcel', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.send('/goto genesis 5,5'))
    expect(h.driver.last('teleport')).toEqual({ kind: 'teleport', realm: DEFAULT_REALM, x: 5, y: 5 })
  })

  it('/goto <world> x,y teleports there (realm-arrival-aware, via the merged teleport request)', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.send('/goto pablo 3,1'))
    expect(h.driver.last('teleport')).toEqual({ kind: 'teleport', realm: 'pablo.dcl.eth', x: 3, y: 1 })
    expect(h.driver.sentOf('changeRealm')).toHaveLength(0)
  })

  it('/goto <world> with no coords still just changes realm (world default spawn)', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.send('/goto pablo'))
    expect(h.driver.last('changeRealm')).toEqual({ kind: 'changeRealm', realm: 'pablo.dcl.eth' })
    expect(h.driver.sentOf('teleport')).toHaveLength(0)
  })

  it('members stream updates the nearby roster', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'members', members: [{ address: '0x1', name: 'Alice' }] })
    expect(h.session().chat.members).toEqual([{ address: '0x1', name: 'Alice' }])
  })
})
