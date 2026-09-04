import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { DiscoverPlace } from '../features/places/placesApi'
import {
  ALL_MARKER_CATEGORIES,
  DEFAULT_VISIBLE_METERS,
  MAX_VISIBLE_METERS,
  MIN_VISIBLE_METERS,
  loadMarkers,
  loadRotation,
  loadStyle,
  loadZoom,
  saveMarkers,
  saveRotation,
  saveStyle,
  saveZoom
} from '../features/minimap/minimapPrefs'
import { placesNear, type MinimapPlace } from '../features/minimap/minimapPlaces'

vi.mock('../features/places/placesApi', () => ({ fetchPlaces: vi.fn() }))

beforeEach(() => localStorage.clear())

describe('minimap prefs — defaults and validation', () => {
  it('falls back to a renderable default when nothing is stored', () => {
    expect(loadStyle()).toBe('satellite')
    expect(loadRotation()).toBe('north')
    expect(loadZoom()).toBe(DEFAULT_VISIBLE_METERS)
    expect(loadMarkers()).toEqual(ALL_MARKER_CATEGORIES)
  })

  it('round-trips a valid value', () => {
    saveStyle('imposters')
    saveRotation('camera')
    saveZoom(128)
    saveMarkers(['poi'])
    expect(loadStyle()).toBe('imposters')
    expect(loadRotation()).toBe('camera')
    expect(loadZoom()).toBe(128)
    expect(loadMarkers()).toEqual(['poi'])
  })

  it('rejects a stale or hand-edited enum rather than rendering nothing', () => {
    localStorage.setItem('dcl-minimap-style', 'hologram')
    localStorage.setItem('dcl-minimap-rotation', 'sideways')
    expect(loadStyle()).toBe('satellite')
    expect(loadRotation()).toBe('north')
  })
})

describe('minimap prefs — zoom clamping', () => {
  it('clamps a stored value that outlived a change to the range', () => {
    localStorage.setItem('dcl-minimap-zoom', '4')
    expect(loadZoom()).toBe(MIN_VISIBLE_METERS)
    localStorage.setItem('dcl-minimap-zoom', '99999')
    expect(loadZoom()).toBe(MAX_VISIBLE_METERS)
  })

  it('falls back on a non-numeric or non-positive value', () => {
    for (const stored of ['not-a-number', '0', '-100', '']) {
      localStorage.setItem('dcl-minimap-zoom', stored)
      expect(loadZoom()).toBe(DEFAULT_VISIBLE_METERS)
    }
  })
})

describe('minimap prefs — markers', () => {
  it('keeps an empty selection: "None" is a choice, not a missing value', () => {
    saveMarkers([])
    expect(loadMarkers()).toEqual([])
  })

  it('drops non-string entries and falls back on malformed JSON', () => {
    localStorage.setItem('dcl-minimap-markers', JSON.stringify(['poi', 7, null, 'art']))
    expect(loadMarkers()).toEqual(['poi', 'art'])
    localStorage.setItem('dcl-minimap-markers', '{ not json')
    expect(loadMarkers()).toEqual(ALL_MARKER_CATEGORIES)
    localStorage.setItem('dcl-minimap-markers', '"poi"') // valid JSON, wrong shape
    expect(loadMarkers()).toEqual(ALL_MARKER_CATEGORIES)
  })

  it('hands back a copy, so a caller mutating it cannot corrupt the defaults', () => {
    const first = loadMarkers()
    first.length = 0
    expect(loadMarkers()).toEqual(ALL_MARKER_CATEGORIES)
  })
})

describe('placesNear', () => {
  const at = (id: string, x: number, y: number): MinimapPlace => ({ id, title: id, categories: [], x, y })

  it('returns places within the radius, nearest first', () => {
    const places = [at('far', 30, 0), at('near', 2, 0), at('mid', 10, 0)]
    expect(placesNear(places, 0, 0, 20).map((p) => p.id)).toEqual(['near', 'mid'])
  })

  it('measures euclidean distance, not per-axis', () => {
    // (3,4) is 5 away: inside a radius of 5, outside a radius of 4.
    expect(placesNear([at('p', 3, 4)], 0, 0, 5)).toHaveLength(1)
    expect(placesNear([at('p', 3, 4)], 0, 0, 4)).toHaveLength(0)
  })

  it('is relative to the given centre, not the origin', () => {
    expect(placesNear([at('p', 100, 100)], 99, 99, 3).map((p) => p.id)).toEqual(['p'])
  })

  it('returns nothing for an empty catalogue', () => {
    expect(placesNear([], 0, 0, 100)).toEqual([])
  })
})

// `centre()` and the world filter aren't exported — drive them through the loader, with the
// places API stubbed. `vi.resetModules()` gives each case a fresh module (the loader memoises
// its in-flight request).
async function loadWith(pages: DiscoverPlace[][]): Promise<MinimapPlace[]> {
  vi.resetModules()
  const api = await import('../features/places/placesApi')
  // The stub survives resetModules, so its call log has to be cleared per case.
  vi.mocked(api.fetchPlaces).mockReset()
  vi.mocked(api.fetchPlaces).mockImplementation(({ offset = 0 } = {}) => {
    const data = pages[offset / 100] ?? []
    return Promise.resolve({ ok: true, total: 0, data })
  })
  const { loadMinimapPlaces } = await import('../features/minimap/minimapPlaces')
  return loadMinimapPlaces()
}

const place = (over: Partial<DiscoverPlace> & { id: string }): DiscoverPlace => ({
  title: over.id,
  description: '',
  image: '',
  positions: [],
  owner: null,
  ...over
})

describe('loadMinimapPlaces', () => {
  it('averages a multi-parcel footprint instead of marking its corner', () => {
    // base_position is the corner, so a marker placed there sits off the actual venue.
    return expect(
      loadWith([[place({ id: 'plaza', positions: ['0,0', '2,0', '0,2', '2,2'], base_position: '0,0' })]])
    ).resolves.toEqual([expect.objectContaining({ id: 'plaza', x: 1, y: 1 })])
  })

  it('falls back to base_position when a place lists no parcels', () => {
    return expect(loadWith([[place({ id: 'solo', positions: [], base_position: '-9,4' })]])).resolves.toEqual([
      expect.objectContaining({ x: -9, y: 4 })
    ])
  })

  it('skips worlds — they are not in Genesis City, so they have no coordinate', () => {
    return expect(
      loadWith([[place({ id: 'w', world: true, positions: ['0,0'] }), place({ id: 'genesis', positions: ['5,5'] })]])
    ).resolves.toEqual([expect.objectContaining({ id: 'genesis' })])
  })

  it('skips a place whose coordinates cannot be parsed', () => {
    return expect(loadWith([[place({ id: 'broken', positions: ['nope'], base_position: 'nope' })]])).resolves.toEqual([])
  })

  it('stops paging on a short page', async () => {
    const full = Array.from({ length: 100 }, (_, i) => place({ id: `p${i}`, positions: ['0,0'] }))
    const places = await loadWith([full, [place({ id: 'last', positions: ['1,1'] })]])
    expect(places).toHaveLength(101)
    const api = await import('../features/places/placesApi')
    expect(vi.mocked(api.fetchPlaces)).toHaveBeenCalledTimes(2)
  })
})
