// DOMAIN: the session's analytics call sites — the sign-in flow reports the screens and clicks
// mobile's lobby does, in order, with the same property shapes (queued; no engine is attached).

import { afterEach, describe, expect, it } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { pending } from '../lib/analytics'
import { DEFAULT_REALM } from '../lib/baseDomain'
import { FakeDriver, renderSession } from './harness'

class LaunchingDriver extends FakeDriver {
  launch(): void {}
}

const entryUrl = location.pathname + location.search
afterEach(() => history.replaceState(null, '', entryUrl))

const names = (from: number): string[] =>
  pending()
    .slice(from)
    .map((e) => `${e.event}:${String(e.properties.button_text ?? e.properties.screen_name)}`)
const loadingEvents = (from: number): unknown[] =>
  pending()
    .slice(from)
    .filter((e) => String(e.properties.screen_name).startsWith('LOADING_'))
    .map((e) => e.properties.extra_properties)

describe('session analytics', () => {
  it('explore as guest: ACCOUNT_HOME → Click PLAY_GUEST → AUTH_SUCCESS guest → LOBBY', async () => {
    const from = pending().length
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    await waitFor(() => expect(h.session().login.busy).toBe(false))

    expect(names(from)).toEqual(['Screen Viewed:ACCOUNT_HOME', 'Click Button:PLAY_GUEST', 'Screen Viewed:AUTH_SUCCESS', 'Screen Viewed:LOBBY'])
    expect(pending()[from + 2].properties).toEqual({ screen_name: 'AUTH_SUCCESS', extra_properties: '{"login_type":"guest"}' })
  })

  it('a friend request reports the receiver', async () => {
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    act(() => h.session().friends.act('request', '0xFRIEND'))

    const last = pending()[pending().length - 1]
    expect(last.event).toBe('Friend Request')
    expect(last.properties).toEqual({ receiver_id: '0xfriend' })
  })

  it("after a link launch, a chat teleport's loading episode names its own parcel, not the link's", async () => {
    history.replaceState(null, '', '/?position=1,2')
    const driver = new LaunchingDriver()
    const from = pending().length
    const h = renderSession({ userId: null }, driver)
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('entering'))
    driver.emit({ kind: 'event', name: 'playerReady' })
    driver.emit({ kind: 'sceneLoading', state: { visible: false, realmConnected: true, title: '', pendingAssets: null } })
    await waitFor(() => expect(h.session().phase).toBe('world'))

    act(() => h.session().chat.send('/goto 50,60'))
    driver.emit({ kind: 'sceneLoading', state: { visible: true, realmConnected: true, title: '', pendingAssets: null } })
    await waitFor(() => expect(h.session().phase).toBe('entering'))
    driver.emit({ kind: 'sceneLoading', state: { visible: false, realmConnected: true, title: '', pendingAssets: null } })
    await waitFor(() => expect(h.session().phase).toBe('world'))

    expect(driver.sent).toContainEqual({ kind: 'teleport', x: 50, y: 60 })
    expect(loadingEvents(from)).toEqual([
      JSON.stringify({ position: '1,2', realm: DEFAULT_REALM, when: 'on_explorer_ready' }),
      JSON.stringify({ position: '1,2', status: 'Success' }),
      JSON.stringify({ position: '50,60', realm: DEFAULT_REALM, when: 'on_teleport' }),
      JSON.stringify({ position: '50,60', status: 'Success' })
    ])
  })
})
