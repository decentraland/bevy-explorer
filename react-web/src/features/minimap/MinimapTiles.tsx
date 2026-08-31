// The minimap's DOM map layer: the imagery for the `satellite` and `parcel` styles, laid out
// in atlas px (see ../map/atlas.ts) at a fixed base scale.
//
// Nothing here reacts to the player moving or the zoom changing — the parent applies both as
// CSS transforms, so this only re-renders when the *set* of tiles changes (crossing a chunk
// boundary), which is every 40 parcels for satellite and every 16 for parcel.

import { PARCELS_PER_TILE, PARCEL_METERS, atlasPx, tileUrl, type Chunk, type ParcelTile } from '../map/atlas'
import styles from './Minimap.module.css'

/**
 * Px per parcel the layer is laid out at. Chosen so the widest zoom only ever downscales and
 * the closest zoom barely upscales — matching the effective max resolution MapPage reaches at
 * full zoom, so both surfaces ask the tile source for the same level of detail.
 */
export const BASE_PX_PER_PARCEL = 32

export function SatelliteTiles({ chunks }: { chunks: Chunk[] }): React.JSX.Element {
  const tilePx = PARCELS_PER_TILE * BASE_PX_PER_PARCEL
  return (
    <>
      {chunks.map(({ col, row }) => (
        <img
          key={`${col},${row}`}
          className={styles.tile}
          src={tileUrl(col, row)}
          alt=""
          draggable={false}
          width={tilePx}
          height={tilePx}
          style={{ left: col * tilePx, top: row * tilePx }}
        />
      ))}
    </>
  )
}

export function ParcelTiles({ tile }: { tile: ParcelTile }): React.JSX.Element {
  // The image spans `meters` square around its centre. Its top-left is the corner at minimum
  // x and *maximum* z, because atlas y grows downward while world z grows upward.
  const half = tile.meters / 2
  const topLeft = atlasPx(tile.centerX - half, tile.centerZ + half, BASE_PX_PER_PARCEL)
  const px = (tile.meters / PARCEL_METERS) * BASE_PX_PER_PARCEL
  return (
    <img
      className={styles.tile}
      src={tile.url}
      alt=""
      draggable={false}
      width={px}
      height={px}
      style={{ left: topLeft.x, top: topLeft.y }}
    />
  )
}
