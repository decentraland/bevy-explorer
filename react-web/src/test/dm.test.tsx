import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: direct messages — DM tabs are HUD state fed by wallet-channel chat lines, per-tab state
// streams, and stored history; sends go to the channel shown.
const BOB = '0x2b6d2d8cd70b5e9548e87f871d4642e0d6387cd7'

describe('dm domain', () => {
  it('a relayed DM opens a tab, watches the partner, and counts unread while not shown', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'hello?', channel: BOB, messageId: '' } })
    const chat = h.session().chat
    expect(chat.channel).toBe('Nearby')
    expect(chat.conversations).toEqual([{ address: BOB, unread: 1, state: null, online: false }])
    expect(h.driver.last('dmWatch')).toEqual({ kind: 'dmWatch', address: BOB, on: true })
    // nothing is read until the tab is shown, and the Nearby view never shows it
    expect(h.driver.last('dmHistory')).toBeUndefined()
    expect(chat.messages.some((m) => m.channel === BOB)).toBe(false)
  })

  it('a System line stays in the Nearby list instead of opening a tab', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: 'system', message: 'Realm set to foo', channel: 'System', messageId: '' } })
    expect(h.session().chat.conversations).toEqual([])
    expect(h.session().chat.messages.map((m) => m.message)).toContain('Realm set to foo')
  })

  it('showing a tab clears its unread, reads its history, and sends to that partner', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'hello?', channel: BOB, messageId: '' } })
    act(() => h.session().chat.select(BOB))
    expect(h.session().chat.channel).toBe(BOB)
    expect(h.session().chat.conversations[0].unread).toBe(0)
    expect(h.driver.last('dmHistory')).toEqual({ kind: 'dmHistory', address: BOB })
    expect(h.session().chat.messages).toEqual([])
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'hello?', receivedAt: 1000, messageId: '', reactions: [] }] })
    const msgs = h.session().chat.messages
    expect(msgs.map((m) => m.message)).toEqual(['hello?'])
    expect(msgs[0].ts).toBe(1000)
    act(() => h.session().chat.send('hi back'))
    expect(h.driver.last('sendChat')).toEqual({ kind: 'sendChat', message: 'hi back', channel: BOB })
  })

  it('a line landing on the shown tab re-reads the store instead of being appended', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.openConversation(BOB))
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'old', receivedAt: 1000, messageId: '', reactions: [] }] })
    const reads = h.driver.sentOf('dmHistory').length
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'live', channel: BOB, messageId: '' } })
    expect(h.driver.sentOf('dmHistory').length).toBe(reads + 1)
    expect(h.session().chat.conversations[0].unread).toBe(0)
    expect(h.session().chat.messages.map((m) => m.message)).toEqual(['old'])
    h.driver.emit({
      kind: 'dmHistory',
      address: BOB,
      entries: [
        { from: BOB, message: 'old', receivedAt: 1000, messageId: '', reactions: [] },
        { from: BOB, message: 'live', receivedAt: 2000, messageId: '', reactions: [] }
      ]
    })
    const msgs = h.session().chat.messages
    expect(msgs.map((m) => m.message)).toEqual(['old', 'live'])
    // the row it already showed keeps its id; only the added row is new
    expect(msgs.map((m) => m.id)).toEqual([0, 1])
  })

  it('nearby traffic does not touch a DM tab', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.openConversation(BOB))
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'old', receivedAt: 1000, messageId: '', reactions: [] }] })
    for (let i = 0; i < 250; i++) {
      h.driver.emit({ kind: 'chat', chat: { sender: '0x1', message: `n${i}`, channel: 'Nearby', messageId: '' } })
    }
    expect(h.session().chat.messages.map((m) => m.message)).toEqual(['old'])
    act(() => h.session().chat.select('Nearby'))
    expect(h.session().chat.messages.length).toBe(200)
  })

  it('reopening chat on a DM tab clears its unread and re-reads it', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.openConversation(BOB))
    act(() => h.session().chat.toggle())
    expect(h.session().chat.open).toBe(false)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'while closed', channel: BOB, messageId: '' } })
    expect(h.session().chat.conversations[0].unread).toBe(1)
    expect(h.session().chat.unread).toBe(1)
    const reads = h.driver.sentOf('dmHistory').length
    act(() => h.session().chat.toggle())
    expect(h.session().chat.conversations[0].unread).toBe(0)
    expect(h.session().chat.unread).toBe(0)
    expect(h.driver.sentOf('dmHistory').length).toBe(reads + 1)
  })

  it('Nearby lines count on the rail while a DM tab is shown, and clear when Nearby is shown', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'chat', chat: { sender: '0x1', message: 'seen', channel: 'Nearby', messageId: '' } })
    expect(h.session().chat.nearbyUnread).toBe(0)
    act(() => h.session().chat.openConversation(BOB))
    h.driver.emit({ kind: 'chat', chat: { sender: '0x1', message: 'missed', channel: 'Nearby', messageId: '' } })
    h.driver.emit({ kind: 'chat', chat: { sender: 'system', message: 'Realm set to foo', channel: 'System', messageId: '' } })
    expect(h.session().chat.nearbyUnread).toBe(2)
    expect(h.session().chat.conversations[0].unread).toBe(0)
    act(() => h.session().chat.select('Nearby'))
    expect(h.session().chat.nearbyUnread).toBe(0)
  })

  it('reopening chat on Nearby clears its rail count', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.toggle())
    h.driver.emit({ kind: 'chat', chat: { sender: '0x1', message: 'while closed', channel: 'Nearby', messageId: '' } })
    expect(h.session().chat.nearbyUnread).toBe(1)
    expect(h.session().chat.unread).toBe(1)
    act(() => h.session().chat.toggle())
    expect(h.session().chat.nearbyUnread).toBe(0)
    expect(h.session().chat.unread).toBe(0)
  })

  it('the sidebar counts lines for other channels while the chat is open but idle', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().chat.open).toBe(true)
    // idle on Nearby: a Nearby line is visible, a DM is not
    h.driver.emit({ kind: 'chat', chat: { sender: '0x1', message: 'seen', channel: 'Nearby', messageId: '' } })
    expect(h.session().chat.unread).toBe(0)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'hidden', channel: BOB, messageId: '' } })
    expect(h.session().chat.unread).toBe(1)
    // active: the rail shows the tab's count, so the sidebar's clears and nothing more counts
    act(() => h.session().chat.setActive(true))
    expect(h.session().chat.unread).toBe(0)
    h.driver.emit({ kind: 'chat', chat: { sender: BOB, message: 'on the rail', channel: BOB, messageId: '' } })
    expect(h.session().chat.unread).toBe(0)
    expect(h.session().chat.conversations[0].unread).toBe(2)
  })

  it('closing a tab drops its lines and a late answer for it', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.openConversation(BOB))
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'hello?', receivedAt: 5, messageId: '', reactions: [] }] })
    expect(h.session().chat.messages.length).toBe(1)
    act(() => h.session().chat.closeConversation(BOB))
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'hello?', receivedAt: 5, messageId: '', reactions: [] }] })
    act(() => h.session().chat.openConversation(BOB))
    expect(h.session().chat.messages).toEqual([])
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'hello?', receivedAt: 5, messageId: '', reactions: [] }] })
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
    act(() => h.session().chat.openConversation(BOB))
    h.driver.emit({ kind: 'dmHistory', address: BOB, entries: [{ from: BOB, message: 'hello?', receivedAt: 5, messageId: '', reactions: [] }] })
    const reads = h.driver.sentOf('dmHistory').length
    act(() => h.session().chat.deleteHistory(BOB))
    expect(h.driver.last('dmDelete')).toEqual({ kind: 'dmDelete', address: BOB })
    expect(h.session().chat.messages).toEqual([])
    // re-read behind the delete, so an answer already queued cannot bring the rows back
    expect(h.driver.sentOf('dmHistory').length).toBe(reads + 1)
  })
})
