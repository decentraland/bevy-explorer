// DOMAIN: the page→engine analytics handoff — events are stamped when they happen, wait for the
// engine's console, leave together as `/track <base64 batch>`, and a rejected send keeps them.

import { beforeEach, describe, expect, it } from 'vitest'
import { attachEngine, dropAttributionParams, encodeBatch, listenForGoogleTag, pending, screenViewed, track, type QueuedEvent } from '../lib/analytics'
import { PREF, setPref } from '../lib/prefs'
import { authLoginUrl } from '../features/auth/sso'

function decodeLine(line: string): QueuedEvent[] {
  const [command, payload] = line.split(' ')
  expect(command).toBe('/track')
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(payload), (c) => c.charCodeAt(0)))) as QueuedEvent[]
}

describe('page analytics', () => {
  beforeEach(() => setPref(PREF.analyticsQueue, ''))

  it('stamps each event with its time, holds it until the engine is attached, then hands everything over at once', async () => {
    const from = pending().length
    screenViewed('ACCOUNT_HOME')
    track('App Opened', { start_kind: 'cold' })
    const queued = pending().slice(from)
    expect(queued.map((e) => e.event)).toEqual(['Screen Viewed', 'App Opened'])
    expect(queued[0].properties).toEqual({ screen_name: 'ACCOUNT_HOME' })
    expect(() => new Date(queued[0].timestamp).toISOString()).not.toThrow()

    const lines: string[] = []
    attachEngine((line) => {
      lines.push(line)
      return Promise.resolve('queued 2')
    })
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(lines).toHaveLength(1)
    expect(decodeLine(lines[0]).map((e) => e.event)).toEqual(queued.map((e) => e.event))
    expect(pending().slice(from)).toEqual([])
  })

  it('keeps a rejected send, in order, for the next one', async () => {
    const sent: string[][] = []
    let rejectOnce = true
    attachEngine((line) => {
      sent.push(decodeLine(line).map((e) => String(e.properties.button_text)))
      if (!rejectOnce) return Promise.resolve('queued')
      rejectOnce = false
      return Promise.reject(new Error('no console'))
    })
    track('Click Button', { button_text: 'a' })
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(pending().map((e) => e.properties.button_text)).toEqual(['a'])
    track('Click Button', { button_text: 'b' })
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(sent).toEqual([['a'], ['a', 'b']])
    expect(pending()).toEqual([])
  })

  it('encodes a batch as one base64 token of its JSON', () => {
    const batch: QueuedEvent[] = [{ event: 'Screen Viewed', properties: { screen_name: 'LOBBY', note: 'ñ' }, timestamp: 't' }]
    const token = encodeBatch(batch)
    expect(token).not.toMatch(/\s/)
    expect(decodeLine(`/track ${token}`)).toEqual(batch)
  })

  it('carries the entry UTMs on the sign-in link, after the return address', () => {
    expect(authLoginUrl('https://play.decentraland.org/?realm=main', [['utm_source', 'ads'], ['utm_campaign', 'q4 launch']])).toBe(
      '/auth/login?redirectTo=https%3A%2F%2Fplay.decentraland.org%2F%3Frealm%3Dmain&utm_source=ads&utm_campaign=q4%20launch'
    )
    expect(authLoginUrl('https://play.decentraland.org/', [])).toBe('/auth/login?redirectTo=https%3A%2F%2Fplay.decentraland.org%2F')
  })

  it('takes the attribution params and the resumed session off the address bar, keeping the rest', () => {
    history.replaceState(null, '', '/?realm=main&utm_source=ads&gclid=g1&fbclid=f1&anon_user_id=abc&analyticsSession=s1&position=0,0')
    dropAttributionParams()
    expect(location.search).toBe('?realm=main&position=0%2C0')
    dropAttributionParams()
    expect(location.search).toBe('?realm=main&position=0%2C0')
    history.replaceState(null, '', '/')
  })

  it("sends GA4's ids from the shell's google tag as Google Tag Linked, and nothing else it gets", async () => {
    const lines: string[] = []
    attachEngine((line) => {
      lines.push(line)
      return Promise.resolve('queued')
    })
    listenForGoogleTag()
    // jsdom's window is its own parent, the shell's place
    const post = (data: unknown, origin = location.origin): void => {
      window.dispatchEvent(new MessageEvent('message', { data, origin, source: window }))
    }
    post({ type: 'bevy-shell:google-tag', client_id: '123.456', session_id: '1792673220' })
    post({ type: 'bevy-shell:google-tag', client_id: '7.8', session_id: null })
    post({ type: 'bevy-shell:google-tag', client_id: '' })
    post({ type: 'bevy-shell:url', search: '' })
    post({ type: 'bevy-shell:google-tag', client_id: '9.9' }, 'https://evil.example')
    await new Promise((resolve) => setTimeout(resolve, 0))
    const sent = lines.flatMap(decodeLine)
    expect(sent.map((e) => [e.event, e.properties])).toEqual([
      ['Google Tag Linked', { ga_client_id: '123.456', ga_session_id: '1792673220' }],
      ['Google Tag Linked', { ga_client_id: '7.8', ga_session_id: null }]
    ])
  })
})
