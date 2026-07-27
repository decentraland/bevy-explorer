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

  // Regression: equipping a saved outfit deploys via setAvatar, which pushes no wearables update, so
  // the bridge re-emits the equipped set it resolved from the outfit's wearables BY URN — independent
  // of the loaded catalog page. Before the fix the session rebuilt the set from `catalog page ∪
  // equipped`, dropping any outfit item that wasn't on the current page; the next single-item equip
  // then deployed without it. Here the outfit's boots are OFF the loaded page: they must still land in
  // the equipped set and survive a later equip. (The mock masks this by re-emitting; the real bridge
  // now does too.)
  it('equipping a saved outfit takes the bridge-resolved set, keeping off-page items', async () => {
    const shirt = { ...wearable('urn:shirt'), category: 'upper_body' }
    // Stale-equipped on the page: the pre-outfit look wears the hat, the outfit doesn't.
    const hat = { ...wearable('urn:hat', true), category: 'hat' }

    const h = renderSession()
    await enterAsGuest(h)
    // Loaded catalog page holds shirt + hat only — the outfit's boots live on another page.
    act(() => h.session().backpack.query({ page: 0, pageSize: 16 }))
    const q = h.driver.last('catalogQuery') as { requestId: number }
    h.driver.emit({ kind: 'catalogPage', catalog: 'wearables', items: [shirt, hat], total: 40, requestId: q.requestId })

    // Equip the outfit — the session only dispatches; it no longer rebuilds the set client-side.
    act(() => h.session().backpack.equipOutfit(0))
    expect(h.driver.last('equipOutfit')).toEqual({ kind: 'equipOutfit', slot: 0 })
    // The bridge resolves the outfit by urn (incl. the off-page boots) and re-emits the full set.
    h.driver.emit({ kind: 'wearables', equipped: [
      { ...wearable('urn:shirt', true), category: 'upper_body' },
      { ...wearable('urn:boots', true), category: 'feet' }
    ] })
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:shirt', 'urn:boots'])
    // The emit also reconciles the page's per-item flags — without it the hat card kept its stale
    // equipped marker (and shadowed the hat slot via page-priority) while the avatar wore no hat.
    expect(h.session().backpack.list.map((w) => [w.urn, w.equipped])).toEqual([
      ['urn:shirt', true],
      ['urn:hat', false]
    ])

    // Now equip a hat (a new category) — before the fix the dropped boots never reached here; the
    // equipped set is authoritative, so equipSetWith keeps them.
    const equipSetWith = (equipped: Wearable[], w: Wearable): string[] =>
      [...equipped.filter((x) => x.category !== w.category).map((x) => x.urn), w.urn]
    act(() => h.session().backpack.equip(equipSetWith(h.session().backpack.equipped, hat)))
    expect(h.driver.last('equip')).toEqual({ kind: 'equip', urns: ['urn:shirt', 'urn:boots', 'urn:hat'] })
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:shirt', 'urn:boots', 'urn:hat'])
  })
})
