// The HUD minimap — the circular map at the top-left, ported from the SDK7 scene's
// mini-map-content. Three styles, matching the old HUD:
//   parcel / satellite — drawn here in the DOM from map tiles (see MinimapTiles)
//   imposters ("Camera") — a live top-down render by the engine, shown through a transparent
//                          cutout (see the `minimap` bridge-scene domain)
//
// Motion is applied as CSS custom properties from a RAF loop rather than by re-rendering:
// the pose arrives ~20/s and the map has to follow it smoothly, so a render per sample would
// cost the whole HUD tree. Every moving part (tile layer, arrow, north label, pins) derives
// its transform from --map-yaw / --map-scale / --map-tx / --map-ty, so one write per frame
// moves all of them. React only re-renders when the tile *set* changes, i.e. on a chunk
// boundary.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ChevronDown, ChevronUp, Gear, Minus, Plus } from '../../design'
import { EngineViewport } from '../engine/EngineViewport'
import { PARCEL_METERS, atlasPx, chunksInRect, parcelTileFor, type Chunk } from '../map/atlas'
import { pinForCategories } from '../map/mapArt'
import { BASE_PX_PER_PARCEL, ParcelTiles, SatelliteTiles } from './MinimapTiles'
import { loadMinimapPlaces, placesNear, type MinimapPlace } from './minimapPlaces'
import {
  MAX_VISIBLE_METERS,
  MIN_VISIBLE_METERS,
  ZOOM_STEP,
  loadMarkers,
  loadRotation,
  loadStyle,
  loadZoom,
  saveMarkers,
  saveRotation,
  saveStyle,
  saveZoom
} from './minimapPrefs'
import { MinimapSettings } from './MinimapSettings'
import type { MapState, MinimapState } from '../session/useEngineSession'
import type { MinimapStyle } from '../../engine/protocol'
import styles from './Minimap.module.css'

/** Diameter of the circle, before --ui-scale. Roughly the 0.25×viewport-height the SDK7 HUD used. */
const MAP_SIZE = 220
/** Zoom easing, in seconds. Interpolated in log space so each step feels the same. */
const ZOOM_TIME = 0.2
/** How far out to mark places, in parcels. Fixed, as in the SDK7 HUD — at the widest zoom the
 *  markers cluster near the middle rather than filling a map you're only glancing at. */
const MARKER_RADIUS = 10

export function Minimap({
  minimap,
  map,
  sceneTitle,
  setEngineViewport
}: {
  minimap: MinimapState
  map: MapState
  sceneTitle: string
  setEngineViewport: (region: 'map' | 'avatarPreview', rect: { x: number; y: number; width: number; height: number } | null) => void
}): React.JSX.Element {
  const [open, setOpen] = useState(true)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [style, setStyle] = useState<MinimapStyle>(loadStyle)
  const [rotation, setRotation] = useState(loadRotation)
  const [visibleMeters, setVisibleMeters] = useState(loadZoom)
  const [markerCategories, setMarkerCategories] = useState(loadMarkers)
  const [places, setPlaces] = useState<MinimapPlace[]>([])
  // Tiles + coords re-render only on a boundary, not per pose sample.
  const [chunks, setChunks] = useState<Chunk[]>([])
  const [parcelTile, setParcelTile] = useState(() => parcelTileFor(0, 0))
  const [parcel, setParcel] = useState({ x: 0, y: 0 })

  const surfaceRef = useRef<HTMLDivElement>(null)
  // Wraps the gear AND its menu: the trigger has to count as "inside", or its own click would
  // close the menu here and immediately reopen it in the button's onClick.
  const settingsRef = useRef<HTMLDivElement>(null)
  // The animated zoom, read by the RAF loop. Mirrors `visibleMeters` but updates every frame
  // during the ease, without a render per frame.
  const zoomRef = useRef(visibleMeters)

  // In a World there are no satellite/parcel tiles, so only the engine-rendered style can show
  // anything. Matches the SDK7 HUD's forceImposters.
  const effectiveStyle: MinimapStyle = minimap.isWorld ? 'imposters' : style

  const { pose, setConfig } = minimap

  // Tell the scene what to render. Only the Camera style needs it; on the DOM styles this is
  // what makes the scene tear its TextureCamera down.
  useEffect(() => {
    setConfig({ style: effectiveStyle, rotation, visibleMeters })
  }, [setConfig, effectiveStyle, rotation, visibleMeters])

  // Place markers. A World's scenes aren't on the Genesis City grid, so there is nothing to mark.
  useEffect(() => {
    if (minimap.isWorld || markerCategories.length === 0) return
    let alive = true
    loadMinimapPlaces()
      .then((p) => {
        if (alive) setPlaces(p)
      })
      .catch((e: unknown) => {
        // Markers are decoration — a failed pull leaves the map itself perfectly usable.
        console.warn('[minimap] could not load place markers', e)
      })
    return () => {
      alive = false
    }
  }, [minimap.isWorld, markerCategories.length])

  const nearby = useMemo(() => {
    // Guard here as well as on the fetch: places loaded in Genesis City stay in state when you
    // travel to a World, and a World's coordinates are local — they sit near the origin, which
    // is exactly where Genesis City has places. Without this you get Genesis markers on a map
    // they have nothing to do with. Kept in state rather than cleared so coming back doesn't refetch.
    if (minimap.isWorld) return []
    return placesNear(places, parcel.x, parcel.y, MARKER_RADIUS).filter((p) =>
      p.categories.some((c) => markerCategories.includes(c))
    )
  }, [minimap.isWorld, places, parcel.x, parcel.y, markerCategories])

  useEffect(() => {
    const surface = surfaceRef.current
    if (surface == null || !open) return
    let raf = 0
    let lastChunkKey = ''
    let lastTileUrl = ''
    let lastParcelKey = ''
    const tick = (): void => {
      raf = requestAnimationFrame(tick)
      const p = pose.current
      const meters = zoomRef.current
      const pxPerParcel = (MAP_SIZE / meters) * PARCEL_METERS
      const scale = pxPerParcel / BASE_PX_PER_PARCEL
      const here = atlasPx(p.x, p.z, BASE_PX_PER_PARCEL)
      // Map rotation is the negation of the heading we're aligning to: CSS rotates clockwise,
      // and turning right must swing the world left. `north` pins it at 0.
      const mapYaw = rotation === 'north' ? 0 : -p.camYaw
      surface.style.setProperty('--map-scale', String(scale))
      surface.style.setProperty('--map-tx', String(-here.x))
      surface.style.setProperty('--map-ty', String(-here.y))
      surface.style.setProperty('--map-yaw', String(mapYaw))
      surface.style.setProperty('--map-arrow', String(p.yaw + mapYaw))

      // Boundary crossings — cheap to test, rare to fire.
      const px = Math.floor(p.x / PARCEL_METERS)
      const py = Math.floor(p.z / PARCEL_METERS)
      const parcelKey = `${px},${py}`
      if (parcelKey !== lastParcelKey) {
        lastParcelKey = parcelKey
        setParcel({ x: px, y: py })
      }
      if (effectiveStyle === 'satellite') {
        // Cull to the circle's *rotated* extent — its bounding box is the circumscribed
        // square, so use the radius in every direction or chunks pop in while turning.
        const r = (MAP_SIZE / 2 / scale) * Math.SQRT2
        const next = chunksInRect(here.x - r, here.y - r, here.x + r, here.y + r, 40 * BASE_PX_PER_PARCEL)
        const key = next.map((c) => `${c.col},${c.row}`).join('|')
        if (key !== lastChunkKey) {
          lastChunkKey = key
          setChunks(next)
        }
      } else if (effectiveStyle === 'parcel') {
        const tile = parcelTileFor(p.x, p.z)
        if (tile.url !== lastTileUrl) {
          lastTileUrl = tile.url
          setParcelTile(tile)
        }
      }
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [pose, rotation, effectiveStyle, open])

  // Zoom: ease in log space so +/- feels uniform across the range, and only commit (render +
  // persist) at the end — the frames in between ride the RAF loop through zoomRef.
  const zoomBy = useCallback((factor: number) => {
    const from = zoomRef.current
    const to = Math.min(MAX_VISIBLE_METERS, Math.max(MIN_VISIBLE_METERS, from * factor))
    if (to === from) return
    const logFrom = Math.log(from)
    const logTo = Math.log(to)
    const start = performance.now()
    const step = (now: number): void => {
      const t = Math.min(1, (now - start) / (ZOOM_TIME * 1000))
      zoomRef.current = Math.exp(logFrom + (logTo - logFrom) * t)
      if (t < 1) {
        requestAnimationFrame(step)
        return
      }
      zoomRef.current = to
      setVisibleMeters(to)
      saveZoom(to)
    }
    requestAnimationFrame(step)
  }, [])

  const canZoomIn = visibleMeters > MIN_VISIBLE_METERS
  const canZoomOut = visibleMeters < MAX_VISIBLE_METERS

  useEffect(() => {
    if (!settingsOpen) return
    const onDown = (e: PointerEvent): void => {
      if (!settingsRef.current?.contains(e.target as Node)) setSettingsOpen(false)
    }
    // Capture: the map underneath opens the full-screen map on click, so this has to win.
    document.addEventListener('pointerdown', onDown, true)
    return () => document.removeEventListener('pointerdown', onDown, true)
  }, [settingsOpen])

  // Every preference persists the moment it changes — the menu has no confirm step.
  const pickStyle = useCallback((s: MinimapStyle) => {
    setStyle(s)
    saveStyle(s)
  }, [])
  const pickRotation = useCallback((r: typeof rotation) => {
    setRotation(r)
    saveRotation(r)
  }, [])
  const pickMarkers = useCallback((categories: string[]) => {
    setMarkerCategories(categories)
    saveMarkers(categories)
  }, [])

  // A click anywhere on the map opens the full-screen map, so the controls layered on top of
  // it must not bubble — otherwise zooming also navigates away.
  const swallow = useCallback((e: React.MouseEvent) => e.stopPropagation(), [])

  const tiles = useMemo(() => {
    if (effectiveStyle === 'satellite') return <SatelliteTiles chunks={chunks} />
    if (effectiveStyle === 'parcel') return <ParcelTiles tile={parcelTile} />
    return null
  }, [effectiveStyle, chunks, parcelTile])

  return (
    <div className={styles.root}>
      <div className={styles.header}>
        {/* No scene deployed on this parcel — say so, rather than implying the lookup failed
            (bevy-ui-scene's widget said "empty scene" for the same case). */}
        <span className={styles.title}>{sceneTitle || 'Empty parcel'}</span>
        <span className={styles.coords}>
          {parcel.x},{parcel.y}
        </span>
        <button
          type="button"
          className={styles.collapse}
          onClick={() => setOpen((v) => !v)}
          aria-label={open ? 'Collapse minimap' : 'Expand minimap'}
          aria-expanded={open}
        >
          {open ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        </button>
      </div>

      {open && (
        <div className={styles.surface} ref={surfaceRef}>
          {/* In Camera style the circle must be see-through: the engine paints the map behind
              the page, and an opaque background here would hide it. */}
          <button
            type="button"
            className={effectiveStyle === 'imposters' ? `${styles.circle} ${styles.cutout}` : styles.circle}
            onClick={map.toggle}
            aria-label="Open map"
          >
            <div className={styles.rotor}>
              <div className={styles.layer}>
                {tiles}
                {nearby.map((p) => {
                  // Anchor at the centre of the place's parcel, not its corner.
                  const at = atlasPx(
                    p.x * PARCEL_METERS + PARCEL_METERS / 2,
                    p.y * PARCEL_METERS + PARCEL_METERS / 2,
                    BASE_PX_PER_PARCEL
                  )
                  return (
                    <span key={p.id} className={styles.poi} style={{ left: at.x, top: at.y }}>
                      <img className={styles.poiPin} src={pinForCategories(p.categories)} alt="" draggable={false} />
                      {/* Only points of interest are named — labelling every category turns the
                          circle into a wall of text, which is why the SDK7 HUD did the same. */}
                      {p.categories.includes('poi') && <span className={styles.poiLabel}>{p.title}</span>}
                    </span>
                  )
                })}
              </div>
              <span className={styles.north} aria-hidden="true">
                <span className={styles.northLabel}>N</span>
              </span>
            </div>

            {/* Camera style: leave the circle transparent and let the engine draw into it.
                Inside the rotor's sibling scope, not the layer — the engine turns its own
                camera, so this must not also be rotated by CSS. */}
            {effectiveStyle === 'imposters' && (
              <span className={styles.cutoutFill}>
                <EngineViewport region="map" report={setEngineViewport} />
              </span>
            )}
            {/* Outlined rather than a flat triangle: a solid --brand arrow disappears over the
                warm satellite imagery around Genesis Plaza. */}
            <svg className={styles.arrow} viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 2 21 22 12 17.4 3 22Z" fill="var(--brand)" stroke="var(--text)" strokeWidth="1.6" strokeLinejoin="round" />
            </svg>
          </button>

          <div className={styles.zoom} onClick={swallow} role="group" aria-label="Minimap zoom">
            <button type="button" className={styles.roundButton} onClick={() => zoomBy(1 / ZOOM_STEP)} disabled={!canZoomIn} aria-label="Zoom in">
              <Plus size={14} />
            </button>
            <button type="button" className={styles.roundButton} onClick={() => zoomBy(ZOOM_STEP)} disabled={!canZoomOut} aria-label="Zoom out">
              <Minus size={14} />
            </button>
          </div>

          <div className={styles.settings} onClick={swallow} ref={settingsRef}>
            <button
              type="button"
              className={styles.roundButton}
              onClick={() => setSettingsOpen((v) => !v)}
              aria-label="Minimap settings"
              aria-expanded={settingsOpen}
            >
              <Gear size={14} />
            </button>
            {settingsOpen && (
              <MinimapSettings
                rotation={rotation}
                onRotation={pickRotation}
                style={style}
                onStyle={pickStyle}
                hideStyle={minimap.isWorld}
                markers={markerCategories}
                onMarkers={pickMarkers}
              />
            )}
          </div>
        </div>
      )}
    </div>
  )
}
