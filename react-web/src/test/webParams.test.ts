// DOMAIN: the engine's web param table as the HUD consumes it (lib/webParams.ts) — the boot config
// is filled from it — and this front-end's launch gate over those params (lib/launchGate.ts).

import { describe, expect, it } from 'vitest'
import { SERVICES, WEB_PARAMS } from '../engine/generated'
import { launchOptionsFromUrl, webParam } from '../lib/webParams'
import { untrustedLaunchParams } from '../lib/launchGate'
import { normaliseServiceUrl } from '../lib/baseDomain'

describe('launchOptionsFromUrl', () => {
  it('reads every launch param, flags by presence, strings verbatim, absent = undefined', () => {
    const q = new URLSearchParams(
      '?preview&portables=a;b&editor&logFps=true&gpuBytesPerFrame=500000&contentServer=https://peer.decentraland.org/content'
    )
    const opts = launchOptionsFromUrl(q)
    // editor is the creator-hub front-end's to set (delivery `host`), never a link's
    expect('editor' in opts).toBe(false)
    expect(opts.preview).toBe(true)
    expect(opts.portables).toBe('a;b')
    expect(opts.imposterSource).toBeUndefined()
    // typed kinds arrive as the engine's type; an unparseable number stays a string for the engine to reject
    expect(opts.logFps).toBe(true)
    expect(opts.gpuBytesPerFrame).toBe(500000)
    expect(opts.contentServer).toBe('https://peer.decentraland.org/content')
    expect(launchOptionsFromUrl(new URLSearchParams('?gpuBytesPerFrame=lots')).gpuBytesPerFrame).toBe('lots')
    // only launch-delivered params: the picker's realm/position and the host-side baseDomain stay out
    for (const name of Object.keys(opts)) expect(webParam(name).delivery).toBe('launch')
    expect(Object.keys(opts).sort()).toEqual(
      WEB_PARAMS.filter((p) => p.delivery === 'launch')
        .map((p) => p.name)
        .sort()
    )
  })
})

describe('normaliseServiceUrl', () => {
  const catalyst = SERVICES.find((s) => s.name === 'catalyst')!
  const socialRpc = SERVICES.find((s) => s.name === 'socialRpc')!
  const pulse = SERVICES.find((s) => s.name === 'pulseServer')!

  it('yields a base url the engine accepts: scheme + host + path, no trailing slash', () => {
    expect(normaliseServiceUrl(catalyst, ' https://peer.example/ ')).toBe('https://peer.example')
    expect(normaliseServiceUrl(catalyst, 'http://127.0.0.1:8799/content/')).toBe('http://127.0.0.1:8799/content')
    expect(normaliseServiceUrl(socialRpc, 'ws://localhost:9000')).toBe('ws://localhost:9000')
  })

  it("drops a bare trailing ? or # — empty to URL.search/hash, a query/fragment to the engine's parser", () => {
    expect(normaliseServiceUrl(catalyst, 'http://127.0.0.1:8799/?')).toBe('http://127.0.0.1:8799')
    expect(normaliseServiceUrl(catalyst, 'http://127.0.0.1:8799/#')).toBe('http://127.0.0.1:8799')
    expect(normaliseServiceUrl(catalyst, 'http://127.0.0.1:8799?#')).toBe('http://127.0.0.1:8799')
  })

  it('takes an authority service as host or host:port, nothing else', () => {
    expect(normaliseServiceUrl(pulse, 'Pulse-Server.decentraland.zone:7777')).toBe('pulse-server.decentraland.zone:7777')
    expect(normaliseServiceUrl(pulse, ' 127.0.0.1 ')).toBe('127.0.0.1')
    expect(normaliseServiceUrl(pulse, '127.0.0.1:80')).toBe('127.0.0.1:80')
    expect(normaliseServiceUrl(pulse, 'https://pulse.example')).toBeNull()
    expect(normaliseServiceUrl(pulse, 'pulse.example:7777/x')).toBeNull()
    expect(normaliseServiceUrl(pulse, 'pulse.example:99999')).toBeNull()
    expect(normaliseServiceUrl(pulse, '')).toBeNull()
  })

  it('is null for anything else, so both sides fall back to the composed default', () => {
    expect(normaliseServiceUrl(catalyst, null)).toBeNull()
    expect(normaliseServiceUrl(catalyst, 'localhost:3000')).toBeNull()
    expect(normaliseServiceUrl(catalyst, 'wss://peer.example')).toBeNull()
    expect(normaliseServiceUrl(socialRpc, 'https://social.example')).toBeNull()
    expect(normaliseServiceUrl(catalyst, 'https://places.example/?x=1')).toBeNull()
    expect(normaliseServiceUrl(catalyst, 'https://places.example/#top')).toBeNull()
  })
})

describe('untrustedLaunchParams', () => {
  it('is empty on a plain entry url', () => {
    expect(untrustedLaunchParams({ native: false })).toEqual([])
  })

  it('gates a content server outside decentraland.org/.zone', () => {
    window.history.replaceState(null, '', '/?contentServer=https://peer.decentraland.zone/content')
    try {
      expect(untrustedLaunchParams({ native: false })).toEqual([])
    } finally {
      window.history.replaceState(null, '', '/?contentServer=http://localhost:8000/content')
    }
    try {
      const [p] = untrustedLaunchParams({ native: false })
      expect(p.name).toBe('contentServer')
      expect(p.warning).toMatch(/Fetches every scene/)
    } finally {
      window.history.replaceState(null, '', '/')
    }
  })

  it('gates a per-service url override outside decentraland.org/.zone, native exempt', () => {
    window.history.replaceState(null, '', '/?places=https://places.decentraland.zone&catalyst=http://localhost:3000')
    try {
      const [p, ...rest] = untrustedLaunchParams({ native: false })
      expect(rest).toEqual([])
      expect(p.name).toBe('catalyst')
      expect(p.warning).toMatch(/"catalyst" backend service/)
      expect(untrustedLaunchParams({ native: true })).toEqual([])
    } finally {
      window.history.replaceState(null, '', '/')
    }
  })

  it('gates an authority service by its host like the rest', () => {
    window.history.replaceState(null, '', '/?pulseServer=pulse-server.decentraland.zone:7777')
    try {
      expect(untrustedLaunchParams({ native: false })).toEqual([])
    } finally {
      window.history.replaceState(null, '', '/?pulseServer=pulse.example')
    }
    try {
      const [p, ...rest] = untrustedLaunchParams({ native: false })
      expect(rest).toEqual([])
      expect(p.name).toBe('pulseServer')
      expect(p.value).toBe('pulse.example')
    } finally {
      window.history.replaceState(null, '', '/')
    }
  })

  it('does not gate a service override the HUD discards (nothing to approve)', () => {
    window.history.replaceState(null, '', '/?catalyst=localhost:3000&places=https://x.example/?q=1')
    try {
      expect(untrustedLaunchParams({ native: false })).toEqual([])
    } finally {
      window.history.replaceState(null, '', '/')
    }
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
