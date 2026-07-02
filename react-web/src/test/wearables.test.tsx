import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { Wearable } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: wearables (backpack) — catalog fetch, preview (non-persisting), equip.
describe('wearables domain', () => {
  const wearable = (urn: string, equipped = false): Wearable => ({
    urn,
    name: urn,
    rarity: 'common',
    category: 'hat',
    equipped
  })

  it('opening the backpack fetches wearables + emotes once', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().backpack.toggle())
    expect(h.driver.sentOf('getWearables')).toHaveLength(1)
    expect(h.driver.sentOf('getEmotes')).toHaveLength(1)
  })

  it('wearables stream populates the catalog', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'wearables', wearables: [wearable('urn:hat'), wearable('urn:hair', true)] })
    expect(h.session().backpack.list).toHaveLength(2)
  })

  it('preview posts previewAvatar; null clears it', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().backpack.preview(['urn:hat']))
    expect(h.driver.last('previewAvatar')).toEqual({ kind: 'previewAvatar', urns: ['urn:hat'] })
    act(() => h.session().backpack.preview(null))
    expect(h.driver.last('previewAvatar')).toEqual({ kind: 'previewAvatar', urns: null })
  })

  it('equip posts the full set and optimistically flips equipped', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'wearables', wearables: [wearable('urn:hat')] })
    act(() => h.session().backpack.equip(['urn:hat']))
    expect(h.driver.last('equip')).toEqual({ kind: 'equip', urns: ['urn:hat'] })
    expect(h.session().backpack.list.find((w) => w.urn === 'urn:hat')?.equipped).toBe(true)
  })
})
