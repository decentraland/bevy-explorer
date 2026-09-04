// DOMAIN: the engine's web param table as the HUD consumes it (lib/webParams.ts) — the boot config
// is filled from it — and this front-end's launch gate over those params (lib/launchGate.ts).

import { describe, expect, it } from 'vitest'
import { WEB_PARAMS } from '../engine/generated'
import { launchOptionsFromUrl, webParam } from '../lib/webParams'
import { untrustedLaunchParams } from '../lib/launchGate'

describe('launchOptionsFromUrl', () => {
  it('reads every launch param, flags by presence, strings verbatim, absent = undefined', () => {
    const q = new URLSearchParams('?pulseServer=localhost:7777&preview&portables=a;b&editor')
    const opts = launchOptionsFromUrl(q)
    // editor is the creator-hub front-end's to set (delivery `host`), never a link's
    expect('editor' in opts).toBe(false)
    expect(opts.pulseServer).toBe('localhost:7777')
    expect(opts.preview).toBe(true)
    expect(opts.portables).toBe('a;b')
    expect(opts.imposterSource).toBeUndefined()
    // only launch-delivered params: the picker's realm/position and the host-side baseDomain stay out
    for (const name of Object.keys(opts)) expect(webParam(name).delivery).toBe('launch')
    expect(Object.keys(opts).sort()).toEqual(
      WEB_PARAMS.filter((p) => p.delivery === 'launch')
        .map((p) => p.name)
        .sort()
    )
  })
})

describe('untrustedLaunchParams', () => {
  it('is empty on a plain entry url', () => {
    expect(untrustedLaunchParams({ native: false })).toEqual([])
  })

  it('reports an unrecognised system scene with the gate copy', () => {
    window.history.replaceState(null, '', '/?systemScene=https://example.com/evil')
    try {
      const [p, ...rest] = untrustedLaunchParams({ native: false })
      expect(rest).toEqual([])
      expect(p.name).toBe('systemScene')
      expect(p.value).toBe('https://example.com/evil')
      expect(p.warning).toMatch(/Replaces the Explorer/)
    } finally {
      window.history.replaceState(null, '', '/')
    }
  })
})
