import { describe, it, expect } from 'vitest'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: nametags are WORLD-SPACE UI rendered by the bridge scene in 3D (billboarded pills pinned
// to each avatar's head). The engine positions them in its render loop, so they track avatars
// smoothly — a DOM port can't, because SDK7 scenes tick below the render rate. They never cross the
// BroadcastChannel, so the React page has no nametag API. This test pins that architectural
// boundary: a full session produces no nametag wire message.
describe('nametags domain (scene-rendered, world-space)', () => {
  it('has no scene↔page API — nothing nametag-related crosses the bridge', async () => {
    const h = renderSession()
    await enterAsGuest(h, { keepSent: true })
    expect(h.driver.sent.some((m) => m.kind.toLowerCase().includes('nametag'))).toBe(false)
  })
})
