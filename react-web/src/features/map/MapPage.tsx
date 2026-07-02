// React Map — full-screen page inside MainMenuShell matching the Unity map: the satellite
// atlas (the same 8×8 tile grid unity-explorer renders) pannable/zoomable, category filter
// chips + search, a player marker, zoom controls, a satellite minimap, a category sidebar
// listing the scenes for the selected category (with their pins on the map), and a place-detail
// panel (places.decentraland.org/api/places) when a parcel/pin is clicked. Teleport via the bridge.

import { useCallback, useEffect, useRef, useState } from 'react'
import { MainMenuShell } from '../menu/MainMenuShell'
import { publicUrl } from '../../lib/publicUrl'
import type { MapState, ProfileState } from '../session/useEngineSession'
import { WorldVisitModal } from '../../components/WorldVisitModal'
import styles from './MapPage.module.css'

// Genesis City satellite atlas — identical source/geometry to unity-explorer's
// SatelliteChunkController (and mobile-curation): an 8×8 grid of 40-parcel jpg chunks.
// File {col}%2C{row}.jpg is the chunk at column `col` (left→right) and row `row` (top→bottom).
const TILE_BASE_URL = 'https://media.githubusercontent.com/media/genesis-city/parcels/new-client-images/maps/lod-0/3/'
const GRID = 8 // 8×8 satellite chunks
const PARCELS_PER_TILE = 40 // one chunk spans 40 parcels
const SPAN = GRID * PARCELS_PER_TILE // 320 parcels across
// Unity places the top-left chunk's center at parcel (-133, 132); chunks are 40 wide, so the
// atlas's top-left corner sits at parcel (-153, 152). x increases right, y increases up.
const ORIGIN_X = -153 // parcel x at the atlas left edge
const ORIGIN_Y = 152 // parcel y at the atlas top edge
const SIZE = 8 // px per parcel in the base (untransformed) atlas

const PLACES_API = 'https://places.decentraland.org/api/places'
const WORLDS_API = 'https://places.decentraland.org/api/worlds'

// Category chips. `api` is the lowercase value the places API expects under `categories`
// (null = no filter / "All"); icons + pins are the unity-explorer textures under /assets/map.
interface Category {
  key: string
  label: string
  api: string | null
}
const CATEGORIES: Category[] = [
  { key: 'all', label: 'All', api: null },
  { key: 'favorites', label: 'Favorites', api: 'favorites' },
  { key: 'social', label: 'Social', api: 'social' },
  { key: 'music', label: 'Music', api: 'music' },
  { key: 'art', label: 'Art', api: 'art' },
  { key: 'game', label: 'Game', api: 'game' },
  { key: 'fashion', label: 'Fashion', api: 'fashion' },
  { key: 'education', label: 'Education', api: 'education' },
  { key: 'shop', label: 'Shop', api: 'shop' },
  { key: 'sports', label: 'Sports', api: 'sports' },
  { key: 'business', label: 'Business', api: 'business' }
]

// Sidebar sort tabs → places-API order_by values (matches unity-explorer's NavmapSearchPlaceSorting).
const SORTS = [
  { key: 'most_active', label: 'MOST ACTIVE' },
  { key: 'like_score', label: 'BEST RATED' },
  { key: 'created_at', label: 'NEWEST' }
]

const catIcon = (key: string): string => `/assets/map/categories/${key}.png`
const catPin = (key: string): string => `/assets/map/pins/${key}.png`

// The 8×8 satellite chunk grid, sized at `size` px per parcel. The container's local origin
// (0,0) is the atlas's top-left corner = parcel (ORIGIN_X, ORIGIN_Y).
function AtlasTiles({ size }: { size: number }): React.JSX.Element {
  const tilePx = PARCELS_PER_TILE * size
  const tiles: React.JSX.Element[] = []
  for (let col = 0; col < GRID; col++) {
    for (let row = 0; row < GRID; row++) {
      tiles.push(
        <img
          key={`${col},${row}`}
          src={`${TILE_BASE_URL}${col}%2C${row}.jpg`}
          alt=""
          draggable={false}
          width={tilePx}
          height={tilePx}
          style={{ position: 'absolute', left: col * tilePx, top: row * tilePx }}
        />
      )
    }
  }
  return (
    <div className={styles.atlas} style={{ width: GRID * tilePx, height: GRID * tilePx }}>
      {tiles}
    </div>
  )
}

interface Place {
  id?: string
  title: string
  description: string
  owner: string | null
  image: string
  base_position: string
  positions: string[]
  user_count: number
  like_rate: number | null
}

// A world (off-atlas realm, e.g. boedo.dcl.eth) from the /api/worlds endpoint.
interface World {
  world_name: string
  title: string
  owner: string | null
  image: string
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

// Right-side list of scenes for the selected category, with MOST ACTIVE / BEST RATED / NEWEST tabs.
function CategorySidebar({
  category,
  sort,
  setSort,
  places,
  loading,
  onClose,
  onSelect
}: {
  category: Category
  sort: string
  setSort: (s: string) => void
  places: Place[]
  loading: boolean
  onClose: () => void
  onSelect: (p: Place) => void
}): React.JSX.Element {
  return (
    <aside className={styles.sidebar}>
      <div className={styles.sidebarHead}>
        <img className={styles.sidebarIcon} src={catIcon(category.key)} alt="" />
        <span className={styles.sidebarTitle}>{category.label}</span>
        <button type="button" className={styles.sidebarClose} aria-label="Close" onClick={onClose}>✕</button>
      </div>
      <div className={styles.sorts}>
        {SORTS.map((s) => (
          <button
            key={s.key}
            type="button"
            className={`${styles.sort} ${sort === s.key ? styles.sortActive : ''}`.trim()}
            onClick={() => setSort(s.key)}
          >
            {s.label}
          </button>
        ))}
      </div>
      <div className={styles.list}>
        {loading && <div className={styles.listMsg}>Loading…</div>}
        {!loading && places.length === 0 && <div className={styles.listMsg}>No scenes found.</div>}
        {!loading &&
          places.map((p) => (
            <button key={p.id ?? p.base_position} type="button" className={styles.card} onClick={() => onSelect(p)}>
              <div className={styles.cardImg} style={{ backgroundImage: p.image ? `url(${p.image})` : undefined }} />
              <div className={styles.cardBody}>
                <div className={styles.cardTitle}>{p.title}</div>
                {p.owner && <div className={styles.cardOwner}>created by {p.owner}</div>}
                <div className={styles.cardStats}>
                  {p.like_rate != null && <span>👍 {Math.round(p.like_rate * 100)}%</span>}
                  <span>👤 {p.user_count}</span>
                </div>
              </div>
              <span className={styles.cardChevron}>›</span>
            </button>
          ))}
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
  const [catKey, setCatKey] = useState('all')
  const [sort, setSort] = useState('most_active')
  const [pan, setPan] = useState({ x: 0, y: 0 })
  const [zoom, setZoom] = useState(1.4)
  const [place, setPlace] = useState<Place | null>(null)
  const [selected, setSelected] = useState<{ x: number; y: number } | null>(null)
  const [list, setList] = useState<Place[]>([])
  const [loading, setLoading] = useState(false)
  const [query, setQuery] = useState('')
  const [worldHits, setWorldHits] = useState<World[]>([])
  const [placeHits, setPlaceHits] = useState<Place[]>([])
  const [confirmWorld, setConfirmWorld] = useState<World | null>(null)
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null)
  const viewRef = useRef<HTMLDivElement>(null)

  const open = map.open
  const category = CATEGORIES.find((c) => c.key === catKey) ?? CATEGORIES[0]
  const sidebarOpen = category.api !== null

  // Place cache keyed by parcel — avoids re-fetching the same place.
  const cache = useRef<Map<string, Place | null>>(new Map())
  const fetchPlace = useCallback(async (px: number, py: number): Promise<void> => {
    const key = `${px},${py}`
    if (cache.current.has(key)) {
      setPlace(cache.current.get(key) ?? null)
      return
    }
    try {
      const res = await fetch(`${PLACES_API}?positions=${key}`)
      const json = (await res.json()) as { data?: Place[] }
      const p = json.data?.[0] ?? null
      cache.current.set(key, p)
      setPlace(p)
    } catch {
      setPlace(null)
    }
  }, [])

  // Load the scenes for the selected category (drives the sidebar list + the map pins).
  useEffect(() => {
    if (!open || category.api === null) {
      setList([])
      return
    }
    const params = new URLSearchParams({ order_by: sort, order: 'desc', limit: '50', with_realms_detail: 'true' })
    if (category.api === 'favorites') params.set('only_favorites', 'true')
    else params.set('categories', category.api)

    const ac = new AbortController()
    setLoading(true)
    fetch(`${PLACES_API}?${params.toString()}`, { signal: ac.signal })
      .then((r) => r.json())
      .then((j: { data?: Place[] }) => {
        setList((j.data ?? []).filter((p) => p.base_position))
        setLoading(false)
      })
      .catch((e) => {
        if (e?.name !== 'AbortError') setLoading(false)
      })
    return () => ac.abort()
  }, [open, category.api, sort])

  // Search places + worlds as the user types. Worlds (e.g. boedo.dcl.eth) aren't on the
  // atlas, so they can't be highlighted — selecting one prompts the travel modal instead.
  useEffect(() => {
    const q = query.trim()
    if (q.length < 2) {
      setWorldHits([])
      setPlaceHits([])
      return
    }
    const ac = new AbortController()
    const t = setTimeout(() => {
      const isEns = /\.eth$/i.test(q)
      const worldParam = isEns ? `names=${encodeURIComponent(q)}` : `search=${encodeURIComponent(q)}`
      const getJson = (url: string): Promise<{ data?: unknown[] }> =>
        fetch(url, { signal: ac.signal }).then((r) => r.json() as Promise<{ data?: unknown[] }>)
      void getJson(`${WORLDS_API}?${worldParam}&limit=12`)
        .then((j) => setWorldHits((j.data as World[]) ?? []))
        .catch(() => undefined)
      void getJson(`${PLACES_API}?search=${encodeURIComponent(q)}&limit=12`)
        .then((j) => setPlaceHits(((j.data as Place[]) ?? []).filter((p) => p.base_position)))
        .catch(() => undefined)
    }, 280)
    return () => {
      clearTimeout(t)
      ac.abort()
    }
  }, [query])

  useEffect(() => {
    if (open) {
      // Center the view on the player's parcel, like Unity's navmap.
      setPan({ x: -zoom * map.x * SIZE, y: zoom * map.y * SIZE })
    } else {
      setPlace(null)
      setSelected(null)
      setCatKey('all')
      setQuery('')
      setConfirmWorld(null)
    }
    // Only re-center when the map opens — player movement shouldn't yank the view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  if (!open) return null

  // Recenter so a parcel sits at the viewport centre (used when picking a search result).
  const centerOn = (px: number, py: number): void => {
    setPan({ x: -zoom * px * SIZE, y: zoom * py * SIZE })
  }
  const openPlace = (pl: Place): void => {
    setPlace(pl)
    const [x, y] = pl.base_position.split(',').map(Number)
    setSelected({ x, y })
  }
  // Picking a place from search: focus it on the map and open its detail.
  const pickPlace = (pl: Place): void => {
    const [x, y] = pl.base_position.split(',').map(Number)
    centerOn(x, y)
    openPlace(pl)
    setQuery('')
  }
  const visitWorld = (w: World): void => {
    map.changeRealm(w.world_name)
    map.toggle()
  }
  const jumpTo = (pl: Place): void => {
    const [x, y] = pl.base_position.split(',').map(Number)
    map.teleport(x, y)
    map.toggle()
  }

  // The whole menu overlay is rendered inside `transform: scale(--ui-scale)` (DPI scaling),
  // so raw mouse pixels must be divided by that scale to land in the atlas's local space.
  const viewScale = (): number => {
    const v = viewRef.current
    if (!v || !v.offsetWidth) return 1
    return v.getBoundingClientRect().width / v.offsetWidth
  }

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
      const s = viewScale()
      setPan((pp) => ({ x: pp.x + dx / s, y: pp.y + dy / s }))
    }
  }
  const onMouseUp = (e: React.MouseEvent): void => {
    const d = drag.current
    drag.current = null
    if (d && !d.moved) {
      // Click → parcel under cursor. Parcel (0,0) sits at the view's centre + pan (local px).
      const view = viewRef.current
      if (!view) return
      const rect = view.getBoundingClientRect()
      const s = view.offsetWidth ? rect.width / view.offsetWidth : 1
      const lx = (e.clientX - rect.left) / s
      const ly = (e.clientY - rect.top) / s
      const cx = view.offsetWidth / 2 + pan.x
      const cy = view.offsetHeight / 2 + pan.y
      const px = Math.round((lx - cx) / (SIZE * zoom))
      const py = Math.round(-(ly - cy) / (SIZE * zoom))
      setSelected({ x: px, y: py })
      void fetchPlace(px, py)
    }
  }
  const onWheel = (e: React.WheelEvent): void => {
    setZoom((z) => Math.min(4, Math.max(0.6, z - e.deltaY * 0.001)))
  }

  // Screen-space position of a parcel (matches the coord-badge / marker math).
  const screenPos = (px: number, py: number): { left: string; top: string } => ({
    left: `calc(50% + ${pan.x + zoom * px * SIZE}px)`,
    top: `calc(50% + ${pan.y - zoom * py * SIZE}px)`
  })

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
            <button
              key={c.key}
              type="button"
              className={`${styles.chip} ${catKey === c.key ? styles.chipActive : ''}`.trim()}
              onClick={() => {
                setCatKey(c.key)
                setPlace(null)
                setSelected(null)
              }}
            >
              <img className={styles.chipIcon} src={catIcon(c.key)} alt="" />
              {c.label}
            </button>
          ))}
          <div className={styles.searchWrap}>
            <input
              className={styles.search}
              placeholder="Search places & worlds"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  if (worldHits[0]) setConfirmWorld(worldHits[0])
                  else if (placeHits[0]) pickPlace(placeHits[0])
                } else if (e.key === 'Escape') setQuery('')
              }}
            />
            {query.trim().length >= 2 && (worldHits.length > 0 || placeHits.length > 0) && (
              <div className={styles.results}>
                {worldHits.map((w) => (
                  <button key={w.world_name} type="button" className={styles.result} onClick={() => setConfirmWorld(w)}>
                    <img className={styles.resultWorldIcon} src={publicUrl('assets/map/world.png')} alt="" />
                    <div className={styles.resultBody}>
                      <div className={styles.resultTitle}>{w.title || w.world_name}</div>
                      <div className={styles.resultSub}>{w.world_name}</div>
                    </div>
                    <span className={styles.resultTag}>WORLD</span>
                  </button>
                ))}
                {placeHits.map((pl) => (
                  <button key={pl.id ?? pl.base_position} type="button" className={styles.result} onClick={() => pickPlace(pl)}>
                    <div className={styles.resultImg} style={{ backgroundImage: pl.image ? `url(${pl.image})` : undefined }} />
                    <div className={styles.resultBody}>
                      <div className={styles.resultTitle}>{pl.title}</div>
                      <div className={styles.resultSub}>{pl.base_position}</div>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        <div
          ref={viewRef}
          data-testid="map-view"
          className={styles.view}
          onMouseDown={onMouseDown}
          onMouseMove={onMouseMove}
          onMouseUp={onMouseUp}
          onMouseLeave={() => (drag.current = null)}
          onWheel={onWheel}
        >
          <div className={styles.atlasWrap} style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}>
            {/* Offset so parcel (0,0) sits at the atlasWrap origin (the pan/zoom anchor). */}
            <div style={{ position: 'absolute', left: -(0 - ORIGIN_X) * SIZE, top: -(ORIGIN_Y - 0) * SIZE }}>
              <AtlasTiles size={SIZE} />
            </div>
            {selected && (
              <div
                className={styles.selected}
                style={{ left: selected.x * SIZE, top: -selected.y * SIZE, width: SIZE, height: SIZE }}
              />
            )}
            <div
              className={styles.marker}
              aria-label="You are here"
              style={{ left: map.x * SIZE, top: -map.y * SIZE }}
            />
          </div>

          {/* Category pins — screen-space so they stay a constant size while panning/zooming. */}
          {list.map((pl) => {
            const [px, py] = pl.base_position.split(',').map(Number)
            return (
              <img
                key={pl.id ?? pl.base_position}
                className={styles.pin}
                src={catPin(category.key)}
                alt={pl.title}
                style={screenPos(px, py)}
                onMouseDown={(e) => e.stopPropagation()}
                onMouseUp={(e) => {
                  e.stopPropagation()
                  drag.current = null
                  openPlace(pl)
                }}
              />
            )
          })}

          {/* Coordinate badge for the selected parcel — screen-space so the text stays crisp. */}
          {selected && (
            <div className={styles.coord} style={screenPos(selected.x, selected.y)}>
              {selected.x}, {selected.y}
            </div>
          )}

          <div className={styles.zoom}>
            <button type="button" onClick={() => setZoom((z) => Math.min(4, z + 0.3))}>+</button>
            <button type="button" onClick={() => setZoom((z) => Math.max(0.6, z - 0.3))}>−</button>
          </div>

          <div className={styles.minimap}>
            <div className={styles.minimapInner}>
              <AtlasTiles size={160 / SPAN} />
            </div>
          </div>
        </div>

        {sidebarOpen && (
          <CategorySidebar
            category={category}
            sort={sort}
            setSort={setSort}
            places={list}
            loading={loading}
            onClose={() => setCatKey('all')}
            onSelect={openPlace}
          />
        )}

        {place && <PlacePanel place={place} onClose={() => setPlace(null)} onJump={() => jumpTo(place)} />}

        {confirmWorld && (
          <WorldVisitModal
            worldName={confirmWorld.world_name}
            title={confirmWorld.title}
            onCancel={() => setConfirmWorld(null)}
            onConfirm={() => visitWorld(confirmWorld)}
          />
        )}
      </div>
    </MainMenuShell>
  )
}
