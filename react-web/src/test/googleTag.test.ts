// DOMAIN: the shell's Google tag for the Ads conversion — what it hands gtag.js (arguments
// objects, consent before config, no page view, the login type, the entry url's campaign and click ids,
// the Segment session on the conversion), that it reports once, and that GA4's ids go back to the app.

// @vitest-environment-options { "url": "https://decentraland.org/play/?realm=main&utm_source=ads&gclid=abc&anon_user_id=z" }
import { describe, expect, it, vi } from 'vitest'
import { onInWorld, reportInWorld } from '../shell/googleTag'

const entries = (): unknown[][] => ((window as Window & { dataLayer?: IArguments[] }).dataLayer ?? []).map((a) => Array.from(a))
const tagScripts = (): HTMLScriptElement[] => Array.from(document.head.querySelectorAll('script[src*="googletagmanager.com/gtag/js"]'))

describe('google tag', () => {
  it('skips previews, then reports the first scene once', () => {
    history.replaceState(null, '', '/play/?preview=true')
    reportInWorld('guest')
    expect(tagScripts()).toHaveLength(0)
    expect(entries()).toEqual([])

    history.replaceState(null, '', '/play/')
    // the app frame's bevy-shell:in-world, as shell/main.ts hands it over
    const app = { postMessage: vi.fn() }
    onInWorld({ type: 'bevy-shell:in-world', login_type: 'guest', session_id: 'segment-session' }, app as unknown as Window)
    expect(tagScripts().map((s) => s.src)).toEqual(['https://www.googletagmanager.com/gtag/js?id=G-GE8LTKK0VK'])
    const sent = entries()
    expect(sent.map((e) => e[0])).toEqual(['consent', 'js', 'set', 'config', 'event', 'get'])
    expect(sent[2]).toEqual(['set', 'user_properties', { login_type: 'guest' }])
    // the url as the visitor arrived, campaign and click ids only: the shell stripped them since
    expect(sent[3]).toEqual(['config', 'G-GE8LTKK0VK', { send_page_view: false, page_location: 'https://decentraland.org/play/?utm_source=ads&gclid=abc' }])
    expect(sent[4]).toEqual(['event', 'bevy_first_move_in_world', { dcl_session_id: 'segment-session' }])

    // gtag.js answering the gets once loaded: the client id, then the session id
    expect(sent[5].slice(0, 3)).toEqual(['get', 'G-GE8LTKK0VK', 'client_id'])
    ;(sent[5][3] as (v: unknown) => void)('123.456')
    const sessionGet = entries()[6]
    expect(sessionGet.slice(0, 3)).toEqual(['get', 'G-GE8LTKK0VK', 'session_id'])
    ;(sessionGet[3] as (v: unknown) => void)(1792673220)
    expect(app.postMessage.mock.calls).toEqual([
      [{ type: 'bevy-shell:google-tag', client_id: '123.456', session_id: '1792673220' }, 'https://decentraland.org']
    ])
    expect(sessionStorage.getItem('dcl-in-world-reported')).toBe('1')

    reportInWorld('fully_registered')
    expect(tagScripts()).toHaveLength(1)
    expect(entries()).toHaveLength(7)
  })
})
