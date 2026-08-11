// Genesis City map tile sources and the coordinate math shared by the full-screen MapPage
// and the HUD minimap.
//
// Two independent tile sources, matching the two DOM minimap styles:
//   satellite — the 8×8 grid of pre-baked jpg chunks (same source/geometry unity-explorer's
//               SatelliteChunkController uses). Fixed extent: outside the grid there is no imagery.
//   parcel    — the coloured parcel atlas rendered on demand by api.decentraland.org, as one
//               image centred near the player.
//
// Coordinates: a parcel is 16 m. Parcel (x, y) maps to world (x*16, z=y*16); parcel y and
// world z increase in the same direction, while screen y grows downward — hence the flip in
// `atlasPx`. "Atlas px" is the untransformed tile-grid space, before any pan/zoom/rotation.

export const PARCEL_METERS = 16

// ---- satellite atlas -------------------------------------------------------

export const TILE_BASE_URL = 'https://media.githubusercontent.com/media/genesis-city/parcels/new-client-images/maps/lod-0/3/'
export const GRID = 8 // 8×8 satellite chunks
export const PARCELS_PER_TILE = 40 // one chunk spans 40 parcels
export const SPAN = GRID * PARCELS_PER_TILE // 320 parcels across
// Unity places the top-left chunk's center at parcel (-133, 132); chunks are 40 wide, so the
// atlas's top-left corner sits at parcel (-153, 152). x increases right, y increases up.
export const ORIGIN_X = -153 // parcel x at the atlas left edge
export const ORIGIN_Y = 152 // parcel y at the atlas top edge
export const SIZE = 8 // px per parcel in the base (untransformed) atlas

/** File {col}%2C{row}.jpg is the chunk at column `col` (left→right), row `row` (top→bottom). */
export function tileUrl(col: number, row: number): string {
  return `${TILE_BASE_URL}${col}%2C${row}.jpg`
}

/** Position of a world-metres point in atlas px, at `size` px per parcel. */
export function atlasPx(worldX: number, worldZ: number, size: number): { x: number; y: number } {
  return {
    x: (worldX / PARCEL_METERS - ORIGIN_X) * size,
    y: (ORIGIN_Y - worldZ / PARCEL_METERS) * size
  }
}

export interface Chunk {
  col: number
  row: number
}

/**
 * The satellite chunks overlapping an axis-aligned window of atlas px, clamped to the grid.
 * The minimap uses this so it renders the handful of chunks it can actually show instead of
 * all 64 — the window must be the *rotated* extent (a circle's bounding box), not the
 * viewport rect, or chunks pop in at the corners while turning.
 */
export function chunksInRect(left: number, top: number, right: number, bottom: number, tilePx: number): Chunk[] {
  const lo = (v: number): number => Math.max(0, Math.min(GRID - 1, Math.floor(v / tilePx)))
  const c0 = lo(left)
  const c1 = lo(right)
  const r0 = lo(top)
  const r1 = lo(bottom)
  const out: Chunk[] = []
  for (let col = c0; col <= c1; col++) {
    for (let row = r0; row <= r1; row++) out.push({ col, row })
  }
  return out
}

// ---- parcel atlas ----------------------------------------------------------

// The API renders on demand, so we key each image to a 16-parcel chunk but request a wider
// 48-parcel span around it. That margin means walking within a chunk never refetches, and the
// image still covers the widest zoom (768 m = 48 parcels) without sampling past its edge.
const PARCEL_TILE_CHUNK = 16
const PARCEL_TILE_PARCELS = 48
const PARCEL_TILE_PX_PER_PARCEL = 16

export interface ParcelTile {
  url: string
  /** Centre of the image, in world metres. */
  centerX: number
  centerZ: number
  /** Extent the image covers, in metres (square). */
  meters: number
}

/** The parcel-atlas image covering a world position. Stable while the player stays in the
 *  same 16-parcel chunk, so it can be used directly as an `<img src>` without thrashing. */
export function parcelTileFor(worldX: number, worldZ: number): ParcelTile {
  const chunkX = Math.floor(worldX / PARCEL_METERS / PARCEL_TILE_CHUNK)
  const chunkY = Math.floor(worldZ / PARCEL_METERS / PARCEL_TILE_CHUNK)
  const centerParcelX = chunkX * PARCEL_TILE_CHUNK + PARCEL_TILE_CHUNK / 2
  const centerParcelY = chunkY * PARCEL_TILE_CHUNK + PARCEL_TILE_CHUNK / 2
  const px = PARCEL_TILE_PARCELS * PARCEL_TILE_PX_PER_PARCEL
  return {
    url: `https://api.decentraland.org/v1/map.png?center=${centerParcelX},${centerParcelY}&width=${px}&height=${px}&size=${PARCEL_TILE_PX_PER_PARCEL}`,
    centerX: centerParcelX * PARCEL_METERS,
    centerZ: centerParcelY * PARCEL_METERS,
    meters: PARCEL_TILE_PARCELS * PARCEL_METERS
  }
}
