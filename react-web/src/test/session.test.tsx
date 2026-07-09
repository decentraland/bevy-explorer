import { describe, it, expect, vi, afterEach } from 'vitest'
import { act, waitFor, render, screen } from '@testing-library/react'
import { renderSession, enterAsGuest, FakeDriver } from './harness'
import { openProfileCard } from '../features/profileCard/ProfileCard'
import { PopupHost, openPopup, resetPopups } from '../design'

// The avatarClick handler opens the world profile card as a popup; stub it so we can assert the call.
vi.mock('../features/profileCard/ProfileCard', () => ({ openProfileCard: vi.fn() }))
afterEach(resetPopups)

// Records launch() so tests can assert the realm/position the engine was booted with.
class LaunchRecordingDriver extends FakeDriver {
  launches: Array<[string?, string?]> = []
  launch(realm?: string, position?: string): void {
    this.launches.push([realm, position])
  }
}

// Simulates a boot-time engine panic: `throwOnLaunch` makes launch() throw synchronously (the generic
// "unreachable" wasm trap), and `panic` is the readable message the iframe stashes and the host reads
// via enginePanic(). Either the sync catch or the post-launch poll must surface it as a FATAL 'launch'
// error rather than the dismissable 'runtime' crash the heartbeat would mislabel it as (gonpombo8's 🔴).
class BootPanicDriver extends FakeDriver {
  throwOnLaunch = false
  panic: { message: string } | null = null
  launch(): void {
    if (this.throwOnLaunch) throw new Error('unreachable')
  }
  enginePanic(): { message: string } | null {
    return this.panic
  }
}

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
    // Jump in shows the picker; the login is deferred until a destination is chosen.
    await waitFor(() => expect(h.session().phase).toBe('picking'))
    act(() => h.session().pickDestination(null))
    await waitFor(() => expect(h.driver.calls).toContain('jumpIn'))
  })

  // A ?realm/?position launch goes straight in — never the Places picker. The /about probe blocks
  // it with a modal on 404 (not found) or on no answer (unreachable).
  async function launchFromUrl(search: string, driver: LaunchRecordingDriver): Promise<ReturnType<typeof renderSession>> {
    history.replaceState(null, '', search)
    const h = renderSession({ userId: null }, driver)
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    return h
  }

  it('a preview ?realm launches straight in when /about answers — no picker', async () => {
    const url = new URL(location.href)
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 200 }))
    try {
      const driver = new LaunchRecordingDriver()
      const h = await launchFromUrl('/?preview=true&realm=http://127.0.0.1:8000&position=0,0', driver)
      await waitFor(() => expect(driver.launches).toHaveLength(1))
      expect(h.session().phase).toBe('entering')
      expect(driver.launches[0]).toEqual(['http://127.0.0.1:8000', '0,0'])
      expect(fetchSpy).toHaveBeenCalledWith('http://127.0.0.1:8000/about')
    } finally {
      fetchSpy.mockRestore()
      history.replaceState(null, '', url.pathname + url.search)
    }
  })

  it('an unreachable ?realm shows the not-reachable modal and never launches', async () => {
    const url = new URL(location.href)
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new TypeError('Failed to fetch'))
    try {
      const driver = new LaunchRecordingDriver()
      const h = await launchFromUrl('/?realm=http://127.0.0.1:9999', driver)
      await waitFor(() =>
        expect(h.session().fatalError).toEqual({
          message: 'The world "http://127.0.0.1:9999" isn\'t reachable right now.',
          source: 'realm'
        })
      )
      expect(driver.launches).toHaveLength(0)
    } finally {
      fetchSpy.mockRestore()
      history.replaceState(null, '', url.pathname + url.search)
    }
  })

  it('a world ?realm whose /about 404s shows World-not-found and never launches', async () => {
    const url = new URL(location.href)
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('', { status: 404 }))
    try {
      const driver = new LaunchRecordingDriver()
      const h = await launchFromUrl('/?realm=nope.dcl.eth', driver)
      await waitFor(() =>
        expect(h.session().fatalError).toEqual({
          message: 'The world "nope.dcl.eth" doesn\'t exist.',
          source: 'realm'
        })
      )
      expect(driver.launches).toHaveLength(0)
      expect(fetchSpy).toHaveBeenCalledWith('https://worlds-content-server.decentraland.org/world/nope.dcl.eth/about')
    } finally {
      fetchSpy.mockRestore()
      history.replaceState(null, '', url.pathname + url.search)
    }
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

  it('a runtime crash from the watchdog sets a dismissable fatal; dismiss re-arms the watchdog', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    // Same-document engine: boot.js's crash watchdog calls window.__onEngineCrash directly.
    act(() => {
      ;(window as Window & { __onEngineCrash?: (m: string, s: string) => void }).__onEngineCrash?.('engine stalled', 'watchdog')
    })
    await waitFor(() => expect(h.session().fatalError).toEqual({ message: 'engine stalled', source: 'runtime' }))
    // Dismiss must re-arm the watchdog (reset its `shown` flag) + clear the stashed panic,
    // else a second genuine crash is swallowed / a stale panic is re-read.
    act(() => h.session().dismissFatal())
    expect(h.session().fatalError).toBeNull()
    expect(h.driver.calls).toContain('rearmCrashWatchdog')
    expect(h.driver.calls).toContain('clearEnginePanic')
  })

  it('a synchronous launch panic sets a FATAL launch error (not the dismissable runtime crash)', async () => {
    const driver = new BootPanicDriver()
    driver.throwOnLaunch = true
    driver.panic = { message: "panicked at inner/mod.rs:41: can't init wasm queue" }
    const h = renderSession({ userId: null }, driver)
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('picking'))
    act(() => h.session().pickDestination(null))
    // launch() threw → the sync catch reads the stashed panic and raises it as fatal 'launch'.
    await waitFor(() =>
      expect(h.session().fatalError).toEqual({
        message: expect.stringContaining("can't init wasm queue"),
        source: 'launch'
      })
    )
  })

  it('a boot panic surfacing after launch returns (async wasm init) is caught by the poll as fatal launch', async () => {
    const driver = new BootPanicDriver()
    driver.panic = { message: 'panicked at OnceCell: already initialized' } // launch returns fine; panic is async
    const h = renderSession({ userId: null }, driver)
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('picking'))
    act(() => h.session().pickDestination(null))
    // launch() returned normally, so the boot-panic poll (250ms) must catch the stashed panic and raise
    // it as fatal 'launch' — not the dismissable 'runtime' crash the heartbeat watchdog would mislabel.
    await waitFor(
      () =>
        expect(h.session().fatalError).toEqual({
          message: expect.stringContaining('already initialized'),
          source: 'launch'
        }),
      { timeout: 2000 }
    )
  })

  it('a nearby-avatar click (avatarClick) opens the profile-card popup at the cursor', () => {
    vi.mocked(openProfileCard).mockClear()
    const h = renderSession({ userId: null })
    // avatarClick carries only the address; the card resolves name/avatar from the roster itself. The
    // handler anchors it at the DOM cursor (0,0 in jsdom, unlocked).
    act(() => h.driver.emit({ kind: 'avatarClick', address: '0xABC' }))
    expect(openProfileCard).toHaveBeenCalledWith('0xABC', 0, 0)
  })

  it("a 'Cancel' system action (the engine's Escape) closes the topmost popup", () => {
    const h = renderSession({ userId: null })
    render(<PopupHost />) // shares the module popup stack
    act(() => {
      openPopup(() => <div>a popup</div>)
    })
    expect(screen.getByText('a popup')).toBeTruthy()
    act(() => h.driver.emit({ kind: 'systemAction', action: 'Cancel' }))
    expect(screen.queryByText('a popup')).toBeNull()
  })

  it('chat.mention opens chat and queues the @name until consumed', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    act(() => h.session().chat.mention('Alice'))
    expect(h.session().chat.open).toBe(true)
    expect(h.session().chat.pendingMention).toBe('Alice')
    act(() => h.session().chat.consumeMention())
    expect(h.session().chat.pendingMention).toBeNull()
  })
})
