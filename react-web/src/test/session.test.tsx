import { describe, it, expect, vi, afterEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { renderSession, enterAsGuest, FakeDriver } from './harness'
import { openProfileCard } from '../features/profileCard/ProfileCard'
import { resetPopups } from '../design'

// The avatarClick handler opens the world profile card as a popup; stub it so we can assert the call.
vi.mock('../features/profileCard/ProfileCard', () => ({ openProfileCard: vi.fn() }))
afterEach(resetPopups)

// Records launch() so tests can assert the realm/position the engine was booted with.
class LaunchRecordingDriver extends FakeDriver {
  launches: Array<[string?, string?]> = []
  hosts: Array<{ holdWorld?: boolean } | undefined> = []
  launch(realm?: string, position?: string, host?: { holdWorld?: boolean }): void {
    this.launches.push([realm, position])
    this.hosts.push(host)
  }
}

// Simulates a boot-time engine panic: `throwOnLaunch` makes launch() throw synchronously (the generic
// "unreachable" wasm trap), and `panic` is the readable message the engine stashes and the host reads
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

  it('the lobby launches holding the world, and a failed trip from it comes back to the lobby', async () => {
    const driver = new LaunchRecordingDriver()
    const h = renderSession({ userId: null }, driver)
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    await waitFor(() => expect(driver.hosts).toEqual([{ holdWorld: true }]))
    await waitFor(() => expect(h.session().login.busy).toBe(false))

    act(() => h.session().pickDestination({ kind: 'parcel', x: 10, y: 20 }))
    const first = driver.sentOf('teleport')[0] as { realm?: string; x: number; y: number; travelId: number }
    expect(first).toMatchObject({ x: 10, y: 20 })
    expect(first.realm).toBeTruthy()
    expect(h.session().phase).toBe('entering')
    act(() => driver.emit({ kind: 'travelResult', travelId: first.travelId, ok: false, realm: first.realm ?? '', message: 'unreachable' }))
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    expect(h.session().travelError).not.toBeNull()

    act(() => h.session().pickDestination(null))
    const second = driver.sentOf('teleport')[1] as { realm?: string; travelId: number }
    act(() => driver.emit({ kind: 'travelResult', travelId: second.travelId, ok: true, realm: second.realm ?? '' }))
    act(() => driver.emit({ kind: 'event', name: 'playerReady' }))
    act(() => driver.emit({ kind: 'sceneLoading', state: { visible: false, realmConnected: true, title: '', pendingAssets: null } }))
    await waitFor(() => expect(h.session().phase).toBe('world'))
    expect(driver.launches).toHaveLength(1)
  })

  it('jump-in reuses the stored login', async () => {
    const h = renderSession({ userId: '0xabc' })
    await waitFor(() => expect(h.session().login.status).toBe('reuse-login-or-new'))
    act(() => h.session().login.jumpIn())
    // Jump in signs in and shows the lobby.
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
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

  it('setInteractableArea posts the HUD inset', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    act(() => h.session().setInteractableArea({ left: 398, top: 0, right: 0, bottom: 0 }))
    expect(h.driver.last('interactableArea')).toEqual({ kind: 'interactableArea', left: 398, top: 0, right: 0, bottom: 0 })
  })

  it('scene-loading / menu / chat-visibility streams update state', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    h.driver.emit({
      kind: 'sceneLoading',
      state: { visible: true, realmConnected: true, title: 'Genesis', pendingAssets: 3 }
    })
    expect(h.session().sceneLoading?.title).toBe('Genesis')
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

  it('the next account after a logout fetches its own data again', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    act(() => h.session().backpack.toggle())
    act(() => h.session().backpack.toggle())
    act(() => h.session().logout())
    await waitFor(() => expect(h.session().phase).toBe('login'))
    h.driver.sent.length = 0
    await enterAsGuest(h, { keepSent: true })
    expect(h.driver.sentOf('getProfile')).toHaveLength(1)
    expect(h.driver.sentOf('getNotifications')).toHaveLength(1)
    act(() => h.session().backpack.toggle())
    expect(h.driver.sentOf('getWearables')).toHaveLength(1)
  })

  it('a chat message leaves the minimap and map slices untouched', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    const { minimap, map } = h.session()
    act(() => h.driver.emit({ kind: 'chat', chat: { sender: '0xabc', message: 'hi', channel: 'Nearby' } }))
    await waitFor(() => expect(h.session().chat.messages).toHaveLength(1))
    expect(h.session().minimap).toBe(minimap)
    expect(h.session().map).toBe(map)
  })

  it('a fetch the bridge failed is asked again on the next open', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    act(() => h.session().backpack.toggle())
    act(() => h.session().backpack.toggle())
    expect(h.driver.sentOf('getWearables')).toHaveLength(1)
    act(() => h.driver.emit({ kind: 'requestFailed', request: 'getWearables', error: 'catalyst down' }))
    act(() => h.session().backpack.toggle())
    expect(h.driver.sentOf('getWearables')).toHaveLength(2)
  })

  it('after a logout, a place picked for the next account waits for that account to spawn', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    act(() => h.session().logout())
    await waitFor(() => expect(h.session().login.status).not.toBe('loading'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    await waitFor(() => expect(h.driver.calls.filter((c) => c === 'loginGuest')).toHaveLength(2))
    await waitFor(() => expect(h.session().login.busy).toBe(false))
    h.driver.sent.length = 0
    act(() => h.session().pickDestination({ kind: 'parcel', x: 10, y: 20 }))
    expect(h.driver.sentOf('teleport')).toHaveLength(0)
    h.driver.emit({ kind: 'event', name: 'playerReady' })
    await waitFor(() => expect(h.driver.sentOf('teleport')).toEqual([{ kind: 'teleport', x: 10, y: 20 }]))
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
    // Jump In launches the engine (holding the world back) behind the lobby.
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
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
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
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

  // The 'Cancel' system action (the engine resolves the cancel key/button; the HUD closes
  // the topmost popup, then panels) is covered in systemActionShortcuts.test.tsx, along
  // with the uiFocus declaration and the pre-world DOM fallback.

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

// DOMAIN: embedded/debug boot modes (lib/bootMode.ts) — the sites `/discover` embed loads
// bevy-web with ?guest=1 (auto guest-login, no sign-in screen) and ?hud=0 (no React HUD);
// ?systemScene= substitutes the super-user ui scene, which then owns login in-engine.
// ?hud=0 is a pure App.tsx render gate; the auto-login flow lives in the hook.
describe('embedded auto-boot (?guest=1 / ?systemScene=)', () => {
  afterEach(() => window.history.replaceState(null, '', '/'))

  it('skips the sign-in screen and enters as a guest without any manual action', async () => {
    window.history.replaceState(null, '', '/?guest=1')
    const h = renderSession({ userId: null })
    // No exploreAsGuest() call in the test — the flag alone drives it. With no
    // URL destination it lands in the lobby (a positioned embed skips that too).
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
  })

  it('with a ?position, drives a guest straight into the world', async () => {
    window.history.replaceState(null, '', '/?guest=1&position=10,20')
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().phase).toBe('entering'))
    // The deferred login runs a paint after the pick, so wait for the call.
    await waitFor(() => expect(h.driver.calls).toContain('loginGuest'))
    h.driver.emit({ kind: 'event', name: 'playerReady' })
    // No loading state received counts as still-loading, so report "done" like the real
    // bridge-scene's stream does (same as harness enterAsGuest).
    h.driver.emit({
      kind: 'sceneLoading',
      state: { visible: false, realmConnected: true, title: '', pendingAssets: null }
    })
    await waitFor(() => expect(h.session().phase).toBe('world'))
  })

  it('?systemScene= boots straight in with no React login at all (the scene owns it)', async () => {
    window.history.replaceState(null, '', '/?systemScene=http://localhost:8100')
    const h = renderSession({ userId: null })
    // No destination params either — the hidden-HUD fallback lands at Genesis 0,0, so the
    // boot never strands on the (invisible) picker.
    await waitFor(() => expect(h.session().phase).toBe('entering'))
    expect(h.driver.calls).not.toContain('loginGuest')
  })

  it('does not auto-login without the flag (normal sign-in screen)', async () => {
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    expect(h.session().phase).toBe('login')
  })
})
