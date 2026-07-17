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

  it('catalogPage populates the grid; the wearables stream carries the equipped set', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    // Grid is server-paginated: query, then the matching page populates the list + total.
    act(() => h.session().backpack.query({ page: 0, pageSize: 16 }))
    const q = h.driver.last('catalogQuery') as { requestId: number }
    expect(q).toMatchObject({ kind: 'catalogQuery', catalog: 'wearables', page: 0 })
    h.driver.emit({ kind: 'catalogPage', catalog: 'wearables', items: [wearable('urn:hat'), wearable('urn:hair', true)], total: 42, requestId: q.requestId })
    expect(h.session().backpack.list).toHaveLength(2)
    expect(h.session().backpack.total).toBe(42)
    // A stale (older requestId) page is ignored.
    h.driver.emit({ kind: 'catalogPage', catalog: 'wearables', items: [], total: 0, requestId: q.requestId - 1 })
    expect(h.session().backpack.list).toHaveLength(2)
    // Equipped set is decoupled from the grid (drives the per-category slots).
    h.driver.emit({ kind: 'wearables', equipped: [wearable('urn:mask', true)] })
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:mask'])
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
    act(() => h.session().backpack.query({ page: 0, pageSize: 16 }))
    const q = h.driver.last('catalogQuery') as { requestId: number }
    h.driver.emit({ kind: 'catalogPage', catalog: 'wearables', items: [wearable('urn:hat')], total: 1, requestId: q.requestId })
    act(() => h.session().backpack.equip(['urn:hat']))
    expect(h.driver.last('equip')).toEqual({ kind: 'equip', urns: ['urn:hat'] })
    // Optimistic flip on the current page.
    expect(h.session().backpack.list.find((w) => w.urn === 'urn:hat')?.equipped).toBe(true)
  })

  // Regression: the real bridge emits `wearables` only once (at load) — it never re-emits after an
  // equip deploy. So `backpack.equipped` must stay authoritative from the optimistic rebuild, or each
  // subsequent equip (built via equipSetWith from the equipped set) drops the previously-equipped
  // items. (The mock masks this by re-emitting; this test never re-emits.)
  it('sequential cross-category equips accumulate without a wearables re-emit', async () => {
    // Mirrors BackpackPage's equipSetWith: keep every equipped item of a different category, add w.
    const equipSetWith = (equipped: Wearable[], w: Wearable): string[] =>
      [...equipped.filter((x) => x.category !== w.category).map((x) => x.urn), w.urn]
    const hat = { ...wearable('urn:hat', true), category: 'hat' }
    const shoes = { ...wearable('urn:shoes'), category: 'feet' }
    const pants = { ...wearable('urn:pants'), category: 'lower_body' }

    const h = renderSession()
    await enterAsGuest(h)
    // Initial equipped set (the one-and-only wearables emit) and a catalog page holding the pool.
    h.driver.emit({ kind: 'wearables', equipped: [hat] })
    act(() => h.session().backpack.query({ page: 0, pageSize: 16 }))
    const q = h.driver.last('catalogQuery') as { requestId: number }
    h.driver.emit({ kind: 'catalogPage', catalog: 'wearables', items: [shoes, pants], total: 2, requestId: q.requestId })

    // Equip shoes (different category) — no re-emit follows.
    act(() => h.session().backpack.equip(equipSetWith(h.session().backpack.equipped, shoes)))
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:hat', 'urn:shoes'])

    // Equip pants (a third category) — before the fix this deployed [hat, pants], dropping shoes.
    act(() => h.session().backpack.equip(equipSetWith(h.session().backpack.equipped, pants)))
    expect(h.driver.last('equip')).toEqual({ kind: 'equip', urns: ['urn:hat', 'urn:shoes', 'urn:pants'] })
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:hat', 'urn:shoes', 'urn:pants'])
    // Categories survive the rebuild, so the per-category slots stay correct.
    expect(h.session().backpack.equipped.map((w) => w.category)).toEqual(['hat', 'feet', 'lower_body'])
  })

  // Regression: equipping a saved outfit deploys via setAvatar and (like every equip) draws no
  // wearables re-emit, so the session must rebuild `backpack.equipped` from the outfit itself. Before
  // the fix it stayed stale and the next single-item equip (equipSetWith) dropped the outfit's other
  // wearables. (The mock masks this by re-emitting; this test never re-emits.)
  it('equipping a saved outfit rebuilds the equipped set so a later equip keeps the outfit', async () => {
    const shirt = { ...wearable('urn:shirt'), category: 'upper_body' }
    const pants = { ...wearable('urn:pants'), category: 'lower_body' }
    const hat = { ...wearable('urn:hat'), category: 'hat' }

    const h = renderSession()
    await enterAsGuest(h)
    // Catalog pool holds every wearable the outfit + later equip resolve against.
    act(() => h.session().backpack.query({ page: 0, pageSize: 16 }))
    const q = h.driver.last('catalogQuery') as { requestId: number }
    h.driver.emit({ kind: 'catalogPage', catalog: 'wearables', items: [shirt, pants, hat], total: 3, requestId: q.requestId })
    // A saved outfit at slot 0 with two wearables.
    h.driver.emit({
      kind: 'outfits',
      metadata: {
        outfits: [
          {
            slot: 0,
            outfit: {
              bodyShape: 'urn:body',
              eyes: { color: { r: 0, g: 0, b: 0 } },
              hair: { color: { r: 0, g: 0, b: 0 } },
              skin: { color: { r: 0, g: 0, b: 0 } },
              wearables: ['urn:shirt', 'urn:pants'],
              forceRender: []
            }
          }
        ],
        namesForExtraSlots: []
      }
    })

    // Equip the outfit — deploys via equipOutfit; no wearables re-emit follows.
    act(() => h.session().backpack.equipOutfit(0))
    expect(h.driver.last('equipOutfit')).toEqual({ kind: 'equipOutfit', slot: 0 })
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:shirt', 'urn:pants'])

    // Now equip a hat (a new category) — before the fix this deployed just [hat], dropping the outfit.
    const equipSetWith = (equipped: Wearable[], w: Wearable): string[] =>
      [...equipped.filter((x) => x.category !== w.category).map((x) => x.urn), w.urn]
    act(() => h.session().backpack.equip(equipSetWith(h.session().backpack.equipped, hat)))
    expect(h.driver.last('equip')).toEqual({ kind: 'equip', urns: ['urn:shirt', 'urn:pants', 'urn:hat'] })
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:shirt', 'urn:pants', 'urn:hat'])
  })
})
