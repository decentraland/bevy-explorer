// DOMAIN: base-domain derivation (lib/baseDomain.ts) — which deployment the HUD keys its
// backend hosts to. The trust side (isTrustedBaseDomain + the launch gate) is covered in
// systemScene.test.tsx.

import { describe, expect, it } from 'vitest'
import { hostBaseDomain } from '../lib/baseDomain'

describe('hostBaseDomain', () => {
  it('derives the apex from decentraland deployments, bare or subdomain', () => {
    expect(hostBaseDomain('decentraland.org')).toBe('decentraland.org')
    expect(hostBaseDomain('play.decentraland.org')).toBe('decentraland.org')
    expect(hostBaseDomain('decentraland.zone')).toBe('decentraland.zone')
    expect(hostBaseDomain('cdn.decentraland.zone')).toBe('decentraland.zone')
  })

  it('derives nothing from unrecognised hosts — the param or the org default decide instead', () => {
    expect(hostBaseDomain('localhost')).toBe(null)
    expect(hostBaseDomain('interconnected.online')).toBe(null)
    expect(hostBaseDomain('decentraland.org.evil.example')).toBe(null)
    expect(hostBaseDomain('evil-decentraland.org')).toBe(null)
  })
})
