import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { AppNotification } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: notifications — list (fetched on entry + on open), unread badge, mark-read.
describe('notifications domain', () => {
  const notif = (id: string, read = false): AppNotification => ({
    id,
    type: 'friendship_request',
    timestamp: '2026-01-01',
    read,
    metadata: {}
  })

  it('fetches notifications on world entry', async () => {
    const h = renderSession()
    await enterAsGuest(h, { keepSent: true })
    expect(h.driver.sentOf('getNotifications')).toHaveLength(1)
  })

  it('re-fetches each time the panel opens', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().notifications.toggle())
    expect(h.driver.sentOf('getNotifications')).toHaveLength(1)
  })

  it('stream updates the list and unread count', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'notifications', notifications: [notif('a'), notif('b', true)] })
    expect(h.session().notifications.list).toHaveLength(2)
    expect(h.session().notifications.unread).toBe(1)
  })

  it('markAllRead posts the unread ids and clears the badge', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'notifications', notifications: [notif('a'), notif('b'), notif('c', true)] })
    act(() => h.session().notifications.markAllRead())
    expect(h.driver.last('markNotificationsRead')).toEqual({
      kind: 'markNotificationsRead',
      ids: ['a', 'b']
    })
    expect(h.session().notifications.unread).toBe(0)
  })

  it('markAllRead is a no-op when nothing is unread', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'notifications', notifications: [notif('a', true)] })
    act(() => h.session().notifications.markAllRead())
    expect(h.driver.sentOf('markNotificationsRead')).toHaveLength(0)
  })
})
