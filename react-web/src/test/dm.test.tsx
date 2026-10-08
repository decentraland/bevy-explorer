import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: direct messages — DM tabs are HUD state fed by wallet-channel chat lines, per-tab state
// streams, and stored history; sends go to the channel shown.
const BOB = '0x2b6d2d8cd70b5e9548e87f871d4642e0d6387cd7'

describe('dm domain', () => {
  it('a relayed DM opens a tab, watches the partner, loads history, and counts unread while not shown', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'hello?', channel: BOB } })
    const chat = h.session().chat
    expect(chat.channel).toBe('Nearby')
    expect(chat.conversations).toEqual([{ address: BOB, unread: 1, state: null, online: false, historyLoaded: false }])
    expect(h.driver.last('dmWatch')).toEqual({ kind: 'dmWatch', address: BOB, on: true })
    expect(h.driver.last('dmHistory')).toEqual({ kind: 'dmHistory', address: BOB })
    // the Nearby view does not show it
    expect(chat.messages.some((m) => m.channel === BOB)).toBe(false)
  })

  it('selecting a tab shows its lines, clears its unread, and sends to that partner', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'hello?', channel: BOB } })
    act(() => h.session().chat.select(BOB))
    const chat = h.session().chat
    expect(chat.channel).toBe(BOB)
    expect(chat.conversations[0].unread).toBe(0)
    expect(chat.messages.map((m) => m.message)).toEqual(['hello?'])
    act(() => h.session().chat.send('hi back'))
    expect(h.driver.last('sendChat')).toEqual({ kind: 'sendChat', message: 'hi back', channel: BOB })
  })

  it('stored history replaces what the tab showed, stamped with its stored time', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    // the DM that opened the tab is already in the store when the history answer comes
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'live', channel: BOB } })
    h.driver.emit({
      kind: 'dmHistory',
      address: BOB,
      entries: [
        { from: BOB, message: 'old', receivedAt: 1000 },
        { from: BOB, message: 'live', receivedAt: 2000 }
      ]
    })
    act(() => h.session().chat.select(BOB))
    const msgs = h.session().chat.messages
    expect(msgs.map((m) => m.message)).toEqual(['old', 'live'])
    expect(msgs[0].ts).toBe(1000)
    expect(msgs.every((m) => m.id < 0)).toBe(true)
    expect(h.session().chat.conversations[0].historyLoaded).toBe(true)
  })

  it('closing a tab drops its lines, so reopening reloads them once', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'hello?', channel: BOB } })
    act(() => h.session().chat.closeConversation(BOB))
    act(() => h.session().chat.openConversation(BOB))
    expect(h.session().chat.messages).toEqual([])
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'hello?', receivedAt: 5 }] })
    expect(h.session().chat.messages.map((m) => m.message)).toEqual(['hello?'])
  })

  it('the partner state updates the tab', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.openConversation(BOB))
    h.driver.emit({ kind: 'dmUserState', address: BOB, state: 'privateMessagesBlocked', online: true })
    expect(h.session().chat.conversations[0]).toMatchObject({ state: 'privateMessagesBlocked', online: true })
  })

  it('openConversation opens chat on the tab; closing it unwatches and returns to Nearby', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.toggle()) // close chat first
    expect(h.session().chat.open).toBe(false)
    act(() => h.session().chat.openConversation(BOB.toUpperCase()))
    expect(h.session().chat.open).toBe(true)
    expect(h.session().chat.channel).toBe(BOB)
    expect(h.session().chat.conversations.map((c) => c.address)).toEqual([BOB])
    act(() => h.session().chat.closeConversation(BOB))
    expect(h.session().chat.conversations).toEqual([])
    expect(h.session().chat.channel).toBe('Nearby')
    expect(h.driver.last('dmWatch')).toEqual({ kind: 'dmWatch', address: BOB, on: false })
  })

  it('deleteHistory asks the engine and clears the tab\'s lines', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'hello?', channel: BOB } })
    act(() => h.session().chat.select(BOB))
    act(() => h.session().chat.deleteHistory(BOB))
    expect(h.driver.last('dmDelete')).toEqual({ kind: 'dmDelete', address: BOB })
    expect(h.session().chat.messages).toEqual([])
  })
})
