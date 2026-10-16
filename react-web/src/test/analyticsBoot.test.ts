// DOMAIN: what a page load reports at boot (lib/analyticsBoot.ts) — the auth redirect's return
// continues the session it left instead of opening a new one, and a campaign link attributes
// this device once.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { analytics } from '../lib/analytics'
import { bootAnalytics } from '../lib/analyticsBoot'
import { PREF } from '../lib/prefs'

const T0 = Date.UTC(2026, 6, 2, 13, 0, 0)

function events(from: number): Array<[string, Record<string, unknown>]> {
  return analytics
    .pending()
    .slice(from)
    .map((e) => [e.event, e.properties])
}

function leftForAuth(session: string, at: number): void {
  localStorage.setItem(PREF.analyticsLastSeen, JSON.stringify({ session, at }))
  localStorage.setItem(PREF.analyticsRedirect, JSON.stringify({ session, at }))
}

describe('boot analytics', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.useFakeTimers({ toFake: ['setInterval'] })
  })
  afterEach(() => vi.useRealTimers())

  it('the auth return within the warm floor keeps the session it left and reports nothing; the marker is spent', () => {
    leftForAuth('session-before-auth', T0)
    const from = analytics.pending().length
    bootAnalytics('?realm=x', T0 + 20_000)
    expect(analytics.sessionId()).toBe('session-before-auth')
    expect(events(from)).toEqual([])

    bootAnalytics('?realm=x', T0 + 25_000)
    expect(events(from).map(([e, p]) => [e, p.start_kind])).toEqual([
      ['App Opened', 'cold'],
      ['Screen Viewed', undefined]
    ])
  })

  it('a slower auth return is a warm open of the same session, with no second START', () => {
    leftForAuth('session-slow-auth', T0)
    const from = analytics.pending().length
    bootAnalytics('', T0 + 95_000)
    expect(analytics.sessionId()).toBe('session-slow-auth')
    expect(events(from)).toEqual([
      ['App Opened', { start_kind: 'warm', trigger: 'icon', prev_session_id: 'session-slow-auth', seconds_since_last_seen: 95 }]
    ])
  })

  it('a return past the auth window is a cold open of a new session that remembers the old one', () => {
    leftForAuth('session-abandoned-auth', T0)
    const from = analytics.pending().length
    bootAnalytics('', T0 + 31 * 60_000)
    expect(analytics.sessionId()).not.toBe('session-abandoned-auth')
    expect(events(from).map(([e, p]) => [e, p.start_kind ?? p.screen_name, p.prev_session_id])).toEqual([
      ['App Opened', 'cold', 'session-abandoned-auth'],
      ['Screen Viewed', 'START', undefined]
    ])
  })

  it('a campaign link attributes the device once, not on every load that carries it', () => {
    const from = analytics.pending().length
    bootAnalytics('?utm_source=x&c=token&position=1,2', T0)
    bootAnalytics('?utm_source=x&c=token&position=1,2', T0 + 60_000)
    const attributions = events(from).filter(([e]) => e === 'Install Attribution')
    expect(attributions).toEqual([
      [
        'Install Attribution',
        {
          referrer: 'utm_source=x&c=token&position=1,2',
          utm_source: 'x',
          campaign_token: 'token',
          click_timestamp: Math.floor(T0 / 1000),
          install_timestamp: Math.floor(T0 / 1000),
          google_play_instant: false,
          attribution_source: 'web_url',
          referrer_settled: true
        }
      ]
    ])
    expect(events(from).filter(([e]) => e === 'App Opened')).toHaveLength(2)
  })
})
