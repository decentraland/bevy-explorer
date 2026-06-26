import { describe, it, expect } from 'vitest'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: nametags — billboarded name labels above avatars. These are WORLD-SPACE UI
// rendered by the engine/bridge scene directly (from PlayerIdentityData); they never
// cross the BroadcastChannel, so the React page has no nametag API. This test pins
// that architectural boundary: a full session emits/produces no nametag wire message.
// (The nearby-roster `members` stream — the closest page-visible data — is covered by
// the chat domain.)
describe('nametags domain (engine-rendered, world-space)', () => {
  it('has no page→scene API — nothing nametag-related crosses the bridge', async () => {
    const h = renderSession()
    await enterAsGuest(h, { keepSent: true })
    expect(h.driver.sent.some((m) => m.kind.toLowerCase().includes('nametag'))).toBe(false)
  })
})
