import { describe, it, expect } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: session — login flow, world-entry fetches, nav actions, engine viewport,
// scene-loading / menu / chat-visibility streams, logout.
describe('session domain', () => {
  it('queries the previous login on mount and lands on the guest screen', async () => {
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    expect(h.driver.calls).toContain('getPreviousLogin')
  })

  it('enter-as-guest: loginGuest console call → entering → world', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h, { keepSent: true })
    expect(h.driver.calls).toContain('loginGuest')
    expect(h.session().phase).toBe('world')
  })

  it('on world entry, fetches profile + notifications', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h, { keepSent: true })
    expect(h.driver.sentOf('getProfile')).toHaveLength(1)
    expect(h.driver.sentOf('getNotifications')).toHaveLength(1)
  })

  it('jump-in reuses the stored login', async () => {
    const h = renderSession({ userId: '0xabc' })
    await waitFor(() => expect(h.session().login.status).toBe('reuse-login-or-new'))
    act(() => h.session().login.jumpIn())
    await waitFor(() => expect(h.driver.calls).toContain('jumpIn'))
  })

  it('nav(mic) posts a navAction', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    act(() => h.session().nav('mic'))
    expect(h.driver.last('navAction')).toEqual({ kind: 'navAction', action: 'mic' })
  })

  it('setEngineViewport posts the carved rect', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    const rect = { x: 1, y: 2, width: 3, height: 4 }
    act(() => h.session().setEngineViewport('map', rect))
    expect(h.driver.last('engineViewport')).toEqual({ kind: 'engineViewport', region: 'map', rect })
  })

  it('scene-loading / menu / chat-visibility streams update state', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    h.driver.emit({
      kind: 'sceneLoading',
      state: { visible: true, realmConnected: true, title: 'Genesis', pendingAssets: 3 }
    })
    expect(h.session().scene?.title).toBe('Genesis')
    h.driver.emit({ kind: 'menuVisibility', open: true })
    expect(h.session().menuOpen).toBe(true)
    h.driver.emit({ kind: 'chatVisibility', open: false })
    expect(h.session().chat.open).toBe(false)
  })

  it('logout returns to the login screen', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    act(() => h.session().logout())
    await waitFor(() => expect(h.session().phase).toBe('login'))
    expect(h.driver.calls).toContain('logout')
  })
})
