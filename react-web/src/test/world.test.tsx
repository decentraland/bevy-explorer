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
