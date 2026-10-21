// DOMAIN: the shell's Google tag for the Ads conversion — what it hands gtag.js (arguments
// objects, consent before config, no page view, the login type) and that it reports once.

// @vitest-environment-options { "url": "https://decentraland.org/play/" }
import { describe, expect, it } from 'vitest'
import { reportInWorld } from '../shell/googleTag'

const entries = (): unknown[][] => ((window as Window & { dataLayer?: IArguments[] }).dataLayer ?? []).map((a) => Array.from(a))
const tagScripts = (): HTMLScriptElement[] => Array.from(document.head.querySelectorAll('script[src*="googletagmanager.com/gtag/js"]'))

describe('google tag', () => {
  it('skips previews, then reports the first scene once', () => {
    history.replaceState(null, '', '/play/?preview=true')
    reportInWorld('guest')
    expect(tagScripts()).toHaveLength(0)
    expect(entries()).toEqual([])

    history.replaceState(null, '', '/play/')
    reportInWorld('guest')
    expect(tagScripts().map((s) => s.src)).toEqual(['https://www.googletagmanager.com/gtag/js?id=G-GE8LTKK0VK'])
    const sent = entries()
    expect(sent.map((e) => e[0])).toEqual(['consent', 'js', 'set', 'config', 'event'])
    expect(sent[2]).toEqual(['set', 'user_properties', { login_type: 'guest' }])
    expect(sent[3]).toEqual(['config', 'G-GE8LTKK0VK', { send_page_view: false }])
    expect(sent[4]).toEqual(['event', 'bevy_first_move_in_world'])
    expect(sessionStorage.getItem('dcl-in-world-reported')).toBe('1')

    reportInWorld('fully_registered')
    expect(tagScripts()).toHaveLength(1)
    expect(entries()).toHaveLength(5)
  })
})
