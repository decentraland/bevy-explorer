import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: avatarPreview — the engine renders the player's avatar (a TextureCamera)
// into a screen rect the React Backpack carves out. The page's only API is reporting
// that rect via engineViewport(region:'avatarPreview', rect|null).
describe('avatarPreview domain', () => {
  it('reports the avatar-preview rect to the scene', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    const rect = { x: 100, y: 50, width: 300, height: 500 }
    act(() => h.session().setEngineViewport('avatarPreview', rect))
    expect(h.driver.last('engineViewport')).toEqual({
      kind: 'engineViewport',
      region: 'avatarPreview',
      rect
    })
  })

  it('clears the rect (null) when the preview closes', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().setEngineViewport('avatarPreview', null))
    expect(h.driver.last('engineViewport')).toEqual({
      kind: 'engineViewport',
      region: 'avatarPreview',
      rect: null
    })
  })
})
