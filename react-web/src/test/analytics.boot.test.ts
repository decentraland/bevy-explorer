// DOMAIN: what a page load reports before any screen shows — App Opened, the entry's campaign as
// Install Attribution (only when the link carries one, stamped with this load's session), then
// START; the return from the sign-in redirect reports none of it again.

import { afterEach, describe, expect, it, vi } from 'vitest'

// the entry is read when the module loads, so each case loads it fresh under its own url
async function bootAt(url: string): Promise<{ event: string; properties: Record<string, unknown> }[]> {
  history.replaceState(null, '', url)
  vi.resetModules()
  const analytics = await import('../lib/analytics')
  const from = analytics.pending().length
  analytics.bootAnalytics()
  session = analytics.sessionId()
  return analytics.pending().slice(from)
}
let session = ''

afterEach(() => history.replaceState(null, '', '/'))

describe('boot analytics', () => {
  it('reports the entry campaign between App Opened and START, raw and in columns', async () => {
    const sent = await bootAt('/?realm=main&utm_source=google&utm_campaign=q4retargeting&utm_term=webtest&gclid=g1&gad_campaignid=42')
    expect(sent.map((e) => e.event)).toEqual(['App Opened', 'Install Attribution', 'Screen Viewed'])
    expect(sent[1].properties).toEqual({
      platform: 'web',
      attribution_source: 'url',
      // this load's session, not the one the engine stamps when a later load sends it
      page_session_id: session,
      referrer: 'utm_source=google&utm_campaign=q4retargeting&utm_term=webtest&gclid=g1&gad_campaignid=42',
      utm_source: 'google',
      utm_medium: null,
      utm_campaign: 'q4retargeting',
      utm_term: 'webtest',
      utm_content: null,
      gclid: 'g1',
      gbraid: null,
      wbraid: null
    })
  })

  it('reports no attribution for a link without a campaign, or with only the cross-domain linker', async () => {
    expect((await bootAt('/?realm=main&position=0,0')).map((e) => e.event)).toEqual(['App Opened', 'Screen Viewed'])
    expect((await bootAt('/?realm=main&_gl=1*abc*_ga*MTIz')).map((e) => e.event)).toEqual(['App Opened', 'Screen Viewed'])
  })

  it('reports nothing again on the return from the sign-in redirect', async () => {
    const sent = await bootAt('/?analyticsSession=s1&utm_source=google')
    expect(sent).toEqual([])
  })
})
