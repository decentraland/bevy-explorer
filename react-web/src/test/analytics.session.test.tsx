// DOMAIN: the session's analytics call sites — the sign-in flow reports the screens and clicks
// mobile's lobby does, in order, with the engine never attached (they just queue).

import { afterEach, describe, expect, it } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { analytics } from '../lib/analytics'
import { DEFAULT_REALM } from '../lib/baseDomain'
import { FakeDriver, renderSession } from './harness'

class LaunchingDriver extends FakeDriver {
  launch(): void {}
}

const entryUrl = location.pathname + location.search
afterEach(() => history.replaceState(null, '', entryUrl))

function tail(from: number): Array<[string, string | undefined, Record<string, unknown>]> {
  return analytics
    .pending()
    .slice(from)
    .map((e) => [e.event, (e.properties.button_text ?? e.properties.screen_name) as string | undefined, e.properties])
}

describe('session analytics', () => {
  it('explore as guest: ACCOUNT_HOME → Click PLAY_GUEST → AUTH_SUCCESS guest → LOBBY (once the welcome is answered)', async () => {
    const from = analytics.pending().length
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    await waitFor(() => expect(h.session().login.busy).toBe(false))

    const events = tail(from)
    expect(events.map(([event, name]) => `${event}:${name}`)).toEqual([
      'Screen Viewed:ACCOUNT_HOME',
      'Click Button:PLAY_GUEST',
      'Screen Viewed:AUTH_SUCCESS',
      'Screen Viewed:LOBBY'
    ])
    expect(events[2][2]).toEqual({ screen_name: 'AUTH_SUCCESS', extra_properties: '{"login_type":"guest"}' })
    for (const e of analytics.pending().slice(from)) expect(e.session_id).toBe(analytics.sessionId())
  })

  it('a link launch that stops for the welcome page is one loading episode: START on_explorer_ready … END Success', async () => {
    history.replaceState(null, '', '/?position=1,2')
    const driver = new LaunchingDriver()
    driver.welcome = { terms: true, newProfile: false }
    const from = analytics.pending().length
    const h = renderSession({ userId: null }, driver)
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('welcome'))
    driver.emit({ kind: 'welcomeAccepted', ok: true })
    await waitFor(() => expect(h.session().phase).toBe('entering'))
    driver.emit({ kind: 'event', name: 'playerReady' })
    driver.emit({ kind: 'sceneLoading', state: { visible: false, realmConnected: true, title: '', pendingAssets: null } })
    await waitFor(() => expect(h.session().phase).toBe('world'))

    const loading = tail(from).filter(([, name]) => name?.startsWith('LOADING_'))
    expect(loading.map(([, , p]) => p)).toEqual([
      { screen_name: 'LOADING_START', extra_properties: JSON.stringify({ position: '1,2', realm: DEFAULT_REALM, when: 'on_explorer_ready' }) },
      { screen_name: 'LOADING_END', extra_properties: JSON.stringify({ position: '1,2', status: 'Success' }) }
    ])
    expect(tail(from).map(([, name]) => name)).toContain('WELCOME')
  })
})
