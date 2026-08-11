import { describe, it, expect } from 'vitest'
import { ORIGIN_X, ORIGIN_Y, PARCEL_METERS, atlasPx, chunksInRect, parcelTileFor, tileUrl } from '../features/map/atlas'

// One satellite chunk spans 40 parcels; at the minimap's 8 px per parcel that's 320 px.
const TILE_PX = 320

describe('atlasPx', () => {
  it('puts the atlas top-left corner at (0, 0) whatever the scale', () => {
    const corner = { x: ORIGIN_X * PARCEL_METERS, z: ORIGIN_Y * PARCEL_METERS }
    expect(atlasPx(corner.x, corner.z, 8)).toEqual({ x: 0, y: 0 })
    expect(atlasPx(corner.x, corner.z, 32)).toEqual({ x: 0, y: 0 })
  })

  it('flips z: parcel y grows north, screen y grows down', () => {
    const at0 = atlasPx(0, 0, 8)
    const oneParcelNorth = atlasPx(0, PARCEL_METERS, 8)
    expect(oneParcelNorth.y).toBe(at0.y - 8)
    expect(oneParcelNorth.x).toBe(at0.x)
  })

  it('does not flip x: world x and screen x grow together', () => {
    const at0 = atlasPx(0, 0, 8)
    expect(atlasPx(PARCEL_METERS, 0, 8).x).toBe(at0.x + 8)
  })

  it('scales linearly with px-per-parcel', () => {
    const small = atlasPx(1600, -800, 8)
    const big = atlasPx(1600, -800, 16)
    expect(big).toEqual({ x: small.x * 2, y: small.y * 2 })
  })
})

describe('chunksInRect', () => {
  it('returns the single chunk a small window sits inside', () => {
    expect(chunksInRect(10, 10, 300, 300, TILE_PX)).toEqual([{ col: 0, row: 0 }])
  })

  it('returns the 2×2 block when the window straddles a chunk boundary', () => {
    const chunks = chunksInRect(TILE_PX - 10, TILE_PX - 10, TILE_PX + 10, TILE_PX + 10, TILE_PX)
    expect(chunks).toHaveLength(4)
    expect(chunks).toEqual(
      expect.arrayContaining([
        { col: 0, row: 0 },
        { col: 0, row: 1 },
        { col: 1, row: 0 },
        { col: 1, row: 1 }
      ])
    )
  })

  it('covers the whole 8×8 grid for a window spanning the atlas', () => {
    expect(chunksInRect(0, 0, TILE_PX * 8 - 1, TILE_PX * 8 - 1, TILE_PX)).toHaveLength(64)
  })

  it('clamps rather than culls: a window off the atlas still yields the nearest edge chunk', () => {
    // Documents current behaviour — outside the grid there is no imagery, so the edge chunk is
    // rendered stretched instead of nothing at all.
    expect(chunksInRect(-5000, -5000, -4000, -4000, TILE_PX)).toEqual([{ col: 0, row: 0 }])
    expect(chunksInRect(99_000, 99_000, 100_000, 100_000, TILE_PX)).toEqual([{ col: 7, row: 7 }])
  })
})

describe('parcelTileFor', () => {
  it('centres the image on the 16-parcel chunk holding the position', () => {
    const tile = parcelTileFor(0, 0)
    // Chunk 0 spans parcels 0..15, so its centre parcel is (8, 8).
    expect(tile.centerX).toBe(8 * PARCEL_METERS)
    expect(tile.centerZ).toBe(8 * PARCEL_METERS)
    expect(tile.url).toContain('center=8,8')
  })

  it('is stable while the player stays in the same chunk', () => {
    const first = parcelTileFor(0, 0)
    const stillInside = parcelTileFor(15 * PARCEL_METERS, 15 * PARCEL_METERS)
    expect(stillInside.url).toBe(first.url)
    // One parcel further is the next chunk, and a different image.
    expect(parcelTileFor(16 * PARCEL_METERS, 0).url).not.toBe(first.url)
  })

  it('floors towards negative coordinates rather than towards zero', () => {
    const tile = parcelTileFor(-PARCEL_METERS, -PARCEL_METERS)
    expect(tile.url).toContain('center=-8,-8')
  })

  it('covers 48 parcels, wide enough for the widest zoom (768 m)', () => {
    expect(parcelTileFor(0, 0).meters).toBe(48 * PARCEL_METERS)
  })
})

describe('tileUrl', () => {
  it('encodes the comma between column and row', () => {
    expect(tileUrl(3, 5)).toMatch(/3%2C5\.jpg$/)
  })
})
