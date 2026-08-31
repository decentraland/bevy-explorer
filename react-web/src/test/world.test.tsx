import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: world — map state (parcel), teleport, microphone toggle/state.
describe('world domain', () => {
  it('opening the map fetches the current parcel', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().map.toggle())
    expect(h.driver.sentOf('getMap')).toHaveLength(1)
  })

  it('mapState stream updates the player parcel', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'mapState', x: -9, y: 12 })
    expect(h.session().map).toMatchObject({ x: -9, y: 12 })
  })

  it('teleport posts target coordinates', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().map.teleport(5, -3))
    expect(h.driver.last('teleport')).toEqual({ kind: 'teleport', x: 5, y: -3 })
  })

  it('sceneInfo drives the minimap title, and re-pushes as the player crosses scenes', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    // The entry overlay's own title (sceneLoading) must not be what the header reads: it is
    // empty once the overlay clears and never changes when walking into the next scene.
    h.driver.emit({ kind: 'sceneLoading', state: { visible: false, realmConnected: true, title: 'Genesis Plaza', pendingAssets: null } })
    expect(h.session().minimap.sceneTitle).toBe('')

    h.driver.emit({ kind: 'sceneInfo', title: 'Genesis Plaza' })
    expect(h.session().minimap.sceneTitle).toBe('Genesis Plaza')

    h.driver.emit({ kind: 'sceneInfo', title: "Make's Stack" })
    expect(h.session().minimap.sceneTitle).toBe("Make's Stack")

    // An undeployed parcel reports no scene — the header falls back to "Empty parcel".
    h.driver.emit({ kind: 'sceneInfo', title: '' })
    expect(h.session().minimap.sceneTitle).toBe('')
  })

  it('mic toggle posts setMic (optimistic) and the stream confirms availability', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().mic.toggle())
    expect(h.driver.last('setMic')).toEqual({ kind: 'setMic', enabled: true })
    expect(h.session().mic.enabled).toBe(true)
    h.driver.emit({ kind: 'mic', enabled: true, available: true })
    expect(h.session().mic).toMatchObject({ enabled: true, available: true })
  })
})
