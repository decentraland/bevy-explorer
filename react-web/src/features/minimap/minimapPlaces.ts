// Place markers for the minimap.
//
// The places API has no "near this parcel" query, so — like the SDK7 HUD did — we pull a
// catalogue once, cache it across sessions, and filter locally by distance. The cache is what
// makes this affordable: without it every world entry would re-pull the whole list.

import { fetchPlaces, type DiscoverPlace } from '../places/placesApi'

const CACHE_KEY = 'dcl-minimap-places'
/** Places move around far less often than a session lasts; a few days keeps this to ~one pull. */
const CACHE_TTL = 3 * 24 * 60 * 60 * 1000
const PAGE = 100
/** Cap the pull at the most-liked 500. Enough to mark everything worth walking to, and it
 *  bounds a cold start at five requests instead of paging the whole of Genesis City. */
const MAX_PLACES = 500

export interface MinimapPlace {
  id: string
  title: string
  categories: string[]
  /** Centre parcel of the place's footprint. */
  x: number
  y: number
}

function parseParcel(s: string): { x: number; y: number } | null {
  const [x, y] = s.split(',').map(Number)
  return Number.isFinite(x) && Number.isFinite(y) ? { x, y } : null
}

/** Centre of a place's footprint. `base_position` is a corner for multi-parcel places, so the
 *  marker would sit off the actual venue — average the parcels instead. */
function centre(place: DiscoverPlace): { x: number; y: number } | null {
  const parcels = (place.positions ?? []).map(parseParcel).filter((p): p is { x: number; y: number } => p != null)
  if (parcels.length === 0) return place.base_position != null ? parseParcel(place.base_position) : null
  const sum = parcels.reduce((a, p) => ({ x: a.x + p.x, y: a.y + p.y }), { x: 0, y: 0 })
  return { x: Math.round(sum.x / parcels.length), y: Math.round(sum.y / parcels.length) }
}

function toMinimapPlaces(data: DiscoverPlace[]): MinimapPlace[] {
  const out: MinimapPlace[] = []
  for (const p of data) {
    // Worlds aren't in Genesis City, so they have no place on a coordinate map.
    if (p.world === true) continue
    const c = centre(p)
    if (c == null) continue
    out.push({ id: p.id, title: p.title, categories: p.categories ?? [], x: c.x, y: c.y })
  }
  return out
}

function readCache(): MinimapPlace[] | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (raw == null) return null
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed == null) return null
    const { at, places } = parsed as { at?: number; places?: unknown }
    if (typeof at !== 'number' || Date.now() - at > CACHE_TTL || !Array.isArray(places)) return null
    return places as MinimapPlace[]
  } catch {
    return null
  }
}

function writeCache(places: MinimapPlace[]): void {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify({ at: Date.now(), places }))
  } catch {
    // ignore quota / privacy-mode failures — we just refetch next session
  }
}

let inFlight: Promise<MinimapPlace[]> | null = null

/** The place catalogue for the markers. Resolves from cache when warm; one shared request
 *  otherwise, so several callers in a session never stack up duplicate pulls. */
export function loadMinimapPlaces(): Promise<MinimapPlace[]> {
  const cached = readCache()
  if (cached != null) return Promise.resolve(cached)
  if (inFlight != null) return inFlight
  inFlight = (async () => {
    const all: MinimapPlace[] = []
    for (let offset = 0; offset < MAX_PLACES; offset += PAGE) {
      const res = await fetchPlaces({ limit: PAGE, offset, order_by: 'like_score_best', order: 'desc' })
      all.push(...toMinimapPlaces(res.data))
      if (res.data.length < PAGE) break
    }
    writeCache(all)
    return all
  })()
  inFlight.catch(() => {
    inFlight = null // let a later attempt retry rather than caching the failure
  })
  return inFlight
}

/** Places whose centre is within `radius` parcels of (x, y), nearest first. */
export function placesNear(places: MinimapPlace[], x: number, y: number, radius: number): MinimapPlace[] {
  return places
    .map((p) => ({ p, d: Math.hypot(p.x - x, p.y - y) }))
    .filter((e) => e.d <= radius)
    .sort((a, b) => a.d - b.d)
    .map((e) => e.p)
}
