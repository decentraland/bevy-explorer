// React Map — full-screen page inside MainMenuShell matching the Unity map: the parcel
// atlas (api.decentraland.org/v1/map.png) pannable/zoomable, category filter chips +
// search, a player marker, zoom controls, a satellite minimap, and a place-detail panel
// (places.decentraland.org/api/places) when a parcel is clicked. Teleport via the bridge.

import { useCallback, useEffect, useRef, useState } from 'react'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { MapState, ProfileState } from '../session/useEngineSession'
import styles from './MapPage.module.css'

// One large atlas image covers the whole Genesis City; we pan/zoom it with CSS.
const SIZE = 9 // px per parcel in the base image
const SPAN = 280 // parcels across (±140) → 2520px image
const ATLAS = SPAN * SIZE
const ATLAS_URL = `https://api.decentraland.org/v1/map.png?center=0,0&width=${ATLAS}&height=${ATLAS}&size=${SIZE}`

const CATEGORIES = ['All', 'Favorites', 'Social', 'Music', 'Art', 'Game', 'Fashion', 'Education', 'Shop', 'Sports', 'Business']

interface Place {
  title: string
  description: string
  owner: string | null
  image: string
  base_position: string
  positions: string[]
  user_count: number
  like_rate: number | null
}

function PlacePanel({ place, onClose, onJump }: { place: Place; onClose: () => void; onJump: () => void }): React.JSX.Element {
  const [bx, by] = place.base_position.split(',')
  return (
    <aside className={styles.place}>
      <button type="button" className={styles.placeClose} aria-label="Back" onClick={onClose}>‹</button>
      <div className={styles.placeImg} style={{ backgroundImage: place.image ? `url(${place.image})` : undefined }} />
      <div className={styles.placeBody}>
        <h2 className={styles.placeTitle}>{place.title}</h2>
        {place.owner && <div className={styles.placeOwner}>created by {place.owner}</div>}
        <div className={styles.placeStats}>
          <span>👤 {place.user_count}</span>
          {place.like_rate != null && <span>👍 {Math.round(place.like_rate * 100)}%</span>}
        </div>
        <button type="button" className={styles.jumpBtn} onClick={onJump}>JUMP IN</button>
        <div className={styles.placeMetaRow}>
          <div>
            <div className={styles.placeMetaLabel}>LOCATION</div>
            <div className={styles.placeMetaValue}>{bx}, {by}</div>
          </div>
          <div>
            <div className={styles.placeMetaLabel}>PARCELS</div>
            <div className={styles.placeMetaValue}>{place.positions.length}</div>
          </div>
        </div>
        <div className={styles.placeMetaLabel}>DESCRIPTION</div>
        <p className={styles.placeDesc}>{place.description}</p>
      </div>
    </aside>
  )
}

export function MapPage({
  map,
  profile,
  onNavigate
}: {
  map: MapState
  profile: ProfileState
  onNavigate: (page: string) => void
}): React.JSX.Element | null {
  const [cat, setCat] = useState('All')
  const [pan, setPan] = useState({ x: 0, y: 0 })
  const [zoom, setZoom] = useState(1.4)
  const [place, setPlace] = useState<Place | null>(null)
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null)
  const viewRef = useRef<HTMLDivElement>(null)

  const open = map.open

  // Place cache keyed by parcel — avoids re-fetching the same place.
  const cache = useRef<Map<string, Place | null>>(new Map())
  const fetchPlace = useCallback(async (px: number, py: number): Promise<void> => {
    const key = `${px},${py}`
    if (cache.current.has(key)) {
      setPlace(cache.current.get(key) ?? null)
      return
    }
    try {
      const res = await fetch(`https://places.decentraland.org/api/places?positions=${key}`)
      const json = (await res.json()) as { data?: Place[] }
      const p = json.data?.[0] ?? null
      cache.current.set(key, p)
      setPlace(p)
    } catch {
      setPlace(null)
    }
  }, [])

  useEffect(() => {
    if (!open) {
      setPlace(null)
      setPan({ x: 0, y: 0 })
    }
  }, [open])

  if (!open) return null

  const onMouseDown = (e: React.MouseEvent): void => {
    drag.current = { x: e.clientX, y: e.clientY, moved: false }
  }
  const onMouseMove = (e: React.MouseEvent): void => {
    const d = drag.current
    if (!d) return
    const dx = e.clientX - d.x
    const dy = e.clientY - d.y
    if (Math.abs(dx) > 3 || Math.abs(dy) > 3) {
      d.moved = true
      d.x = e.clientX
      d.y = e.clientY
      setPan((pp) => ({ x: pp.x + dx, y: pp.y + dy }))
    }
  }
  const onMouseUp = (e: React.MouseEvent): void => {
    const d = drag.current
    drag.current = null
    if (d && !d.moved) {
      // Click → parcel under cursor. Image centre (0,0) sits at viewport centre + pan.
      const view = viewRef.current
      if (!view) return
      const rect = view.getBoundingClientRect()
      const cx = rect.width / 2 + pan.x
      const cy = rect.height / 2 + pan.y
      const px = Math.round((e.clientX - rect.left - cx) / (SIZE * zoom))
      const py = Math.round(-(e.clientY - rect.top - cy) / (SIZE * zoom))
      void fetchPlace(px, py)
    }
  }
  const onWheel = (e: React.WheelEvent): void => {
    setZoom((z) => Math.min(4, Math.max(0.6, z - e.deltaY * 0.001)))
  }

  const p = profile.data
  return (
    <MainMenuShell
      active="map"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={map.toggle}
    >
      <div className={styles.wrap}>
        <div className={styles.chips}>
          {CATEGORIES.map((c) => (
            <button key={c} type="button" className={`${styles.chip} ${cat === c ? styles.chipActive : ''}`.trim()} onClick={() => setCat(c)}>
              {c}
            </button>
          ))}
          <input className={styles.search} placeholder="Search" />
        </div>

        <div
          ref={viewRef}
          className={styles.view}
          onMouseDown={onMouseDown}
          onMouseMove={onMouseMove}
          onMouseUp={onMouseUp}
          onMouseLeave={() => (drag.current = null)}
          onWheel={onWheel}
        >
          <div className={styles.atlasWrap} style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}>
            <img className={styles.atlas} src={ATLAS_URL} alt="Decentraland map" draggable={false} width={ATLAS} height={ATLAS} />
            <div className={styles.marker} aria-label="You are here" />
          </div>

          <div className={styles.zoom}>
            <button type="button" onClick={() => setZoom((z) => Math.min(4, z + 0.3))}>+</button>
            <button type="button" onClick={() => setZoom((z) => Math.max(0.6, z - 0.3))}>−</button>
          </div>

          <div className={styles.minimap}>
            <img src={`https://api.decentraland.org/v1/map.png?center=0,0&width=320&height=200&size=2`} alt="" width={160} height={100} />
          </div>
        </div>

        {place && <PlacePanel place={place} onClose={() => setPlace(null)} onJump={() => { const [x, y] = place.base_position.split(',').map(Number); map.teleport(x, y); map.toggle() }} />}
      </div>
    </MainMenuShell>
  )
}
