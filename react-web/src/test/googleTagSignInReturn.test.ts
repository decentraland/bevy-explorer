// DOMAIN: the shell's Google tag after the sign-in redirect — the page comes back without the
// campaign and click ids it was opened with, and the conversion is still credited to them (a signed-in
// player, so login_type web3).

// @vitest-environment-options { "url": "https://decentraland.org/play/?realm=main" }
import { describe, expect, it, vi } from 'vitest'

vi.hoisted(() => sessionStorage.setItem('dcl-entry-campaign', 'utm_source=ads&gclid=abc'))

import { reportInWorld } from '../shell/googleTag'

describe('google tag after the sign-in redirect', () => {
  it('reports the campaign the tab arrived with', () => {
    reportInWorld('fully_registered')
    const sent = ((window as Window & { dataLayer?: IArguments[] }).dataLayer ?? []).map((a) => Array.from(a))
    expect(sent.find((e) => e[0] === 'set')).toEqual(['set', 'user_properties', { login_type: 'web3' }])
    expect(sent.find((e) => e[0] === 'config')).toEqual([
      'config',
      'G-GE8LTKK0VK',
      { send_page_view: false, page_location: 'https://decentraland.org/play/?utm_source=ads&gclid=abc' }
    ])
  })
})
