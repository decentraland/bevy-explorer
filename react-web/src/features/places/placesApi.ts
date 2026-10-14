// Typed client for the Decentraland Places API (places.decentraland.org/api).
// Plain fetch; the pure helpers below derive card fields from a DiscoverPlace and
// are kept side-effect free so they're unit-testable.

import { serviceUrl } from '../../lib/baseDomain'

const API_BASE = `${serviceUrl('places')}/api`

export interface DiscoverPlace {
  id: string
  title: string
  description: string
  image: string
  positions: string[]
  base_position?: string
  owner: string | null
  contact_name?: string
  categories?: string[]
  favorites?: number
  likes?: number
  dislikes?: number
  user_count?: number
  user_name?: string
  world?: boolean
  world_name?: string
  /** In the places API's curated set — the Featured rail. */
  highlighted?: boolean
  live?: boolean
  live_event_name?: string | null
}

export type PlacesOrderBy = 'most_active' | 'name' | 'created_at' | 'updated_at' | 'like_score_best'
export type PlacesOrder = 'asc' | 'desc'

export interface PlacesResponse {
  ok: boolean
  total: number
  data: DiscoverPlace[]
}

export interface FetchPlacesArgs {
  limit?: number
  offset?: number
  order_by?: PlacesOrderBy
  order?: PlacesOrder
  search?: string
  /** Only points of interest (featured). */
  only_pois?: boolean
  categories?: string[]
  /** Filter to a creator address (My Places). */
  owner?: string
}

function buildParams(args: FetchPlacesArgs, withCategories: boolean): URLSearchParams {
  const params = new URLSearchParams()
  params.set('limit', String(args.limit ?? 24))
  params.set('offset', String(args.offset ?? 0))
  params.set('order_by', args.order_by ?? 'most_active')
  params.set('order', args.order ?? 'desc')
  if (args.search) params.set('search', args.search)
  if (args.owner) params.set('owner', args.owner) // creator filter — applies to places and worlds alike
  if (withCategories) {
    if (args.only_pois) params.set('only_pois', 'true')
    for (const c of args.categories ?? []) params.append('categories', c)
  }
  return params
}

async function getPlaces(path: string, params: URLSearchParams): Promise<PlacesResponse> {
  const res = await fetch(`${API_BASE}${path}?${params.toString()}`)
  if (!res.ok) throw new Error(`places API ${path} failed: ${res.status}`)
  const json = (await res.json()) as Partial<PlacesResponse>
  if (json.ok !== true || !Array.isArray(json.data)) throw new Error(`places API ${path} returned not-ok`)
  return { ok: true, total: json.total ?? json.data.length, data: json.data }
}

// ---- request cache ----
// The Places page unmounts when closed, so each open would otherwise refetch a list we just had.
// Memoize each endpoint response by its full URL for a few minutes; usePlaces reads it synchronously
// on mount so the grid paints instantly (no spinner). prefetchPlaces() warms the default entry during
// login so the post-jump-in picker is instant too. Cached promises run to completion (no abort) — the
// consumer guards against stale results itself rather than aborting a promise others may share.
const CACHE_TTL = 5 * 60_000
const cache = new Map<string, { at: number; promise: Promise<PlacesResponse> }>()

function cachedGet(path: string, params: URLSearchParams): Promise<PlacesResponse> {
  const url = `${path}?${params.toString()}`
  const hit = cache.get(url)
  if (hit && Date.now() - hit.at < CACHE_TTL) return hit.promise
  const promise = getPlaces(path, params)
  cache.set(url, { at: Date.now(), promise })
  promise.catch(() => {
    if (cache.get(url)?.promise === promise) cache.delete(url) // don't cache a failure
  })
  return promise
}

export function fetchPlaces(args: FetchPlacesArgs = {}): Promise<PlacesResponse> {
  return cachedGet('/places', buildParams(args, true))
}

// Worlds take the same shape but no category / poi filters.
export function fetchWorlds(args: FetchPlacesArgs = {}): Promise<PlacesResponse> {
  return cachedGet('/worlds', buildParams(args, false))
}

// ---- destinations ----
// The feed decentraland.org/places and the desktop client read: places AND worlds in one list,
// ordered `highlighted DESC, user_count DESC`, with live presence when with_realms_detail is set.
// It ignores every order_by but most_active, and only matches searches of 3+ characters.

export interface FetchDestinationsArgs {
  limit?: number
  offset?: number
  order_by?: 'most_active'
  search?: string
  categories?: string[]
  owner?: string
  only_highlighted?: boolean
}

export function fetchDestinations(args: FetchDestinationsArgs = {}): Promise<PlacesResponse> {
  const params = new URLSearchParams()
  params.set('limit', String(args.limit ?? 24))
  params.set('offset', String(args.offset ?? 0))
  if (args.order_by) params.set('order_by', args.order_by)
  const q = args.search?.trim()
  if (q && q.length >= 3) params.set('search', q)
  if (args.owner) params.set('owner', args.owner)
  if (args.only_highlighted) params.set('only_highlighted', 'true')
  params.set('with_realms_detail', 'true')
  params.set('with_live_events', 'true')
  for (const c of args.categories ?? []) params.append('categories', c)
  return cachedGet('/destinations', params)
}

export const BROWSE_ARGS = { limit: 48, offset: 0, order_by: 'most_active' as const }
// The feed puts every highlighted row first, so the top four busiest need a page well past them.
export const LIVE_FEED_ARGS = { limit: 40, offset: 0, order_by: 'most_active' as const }
export const FEATURED_ARGS = { limit: 100, offset: 0, only_highlighted: true }

/** Warm the default Places view during login so the picker paints from cache (the cache dedupes). */
export function prefetchPlaces(): void {
  void fetchDestinations(BROWSE_ARGS).catch(() => {})
  void fetchDestinations(LIVE_FEED_ARGS).catch(() => {})
  void fetchDestinations(FEATURED_ARGS).catch(() => {})
}

// ---- pure, side-effect-free helpers (unit-tested) ----

export function placeCoords(p: DiscoverPlace): string {
  return p.world ? (p.world_name ?? '') : (p.base_position ?? p.positions?.[0] ?? '')
}

export function placeRating(p: DiscoverPlace): number {
  const likes = p.likes
  const dislikes = p.dislikes
  if (likes == null || dislikes == null) return 0
  const total = likes + dislikes
  if (total === 0) return 0
  return Math.round((likes / total) * 100)
}

export function placePlayers(p: DiscoverPlace): number {
  return p.user_count ?? 0
}

export function placeCreator(p: DiscoverPlace): string {
  return p.user_name || (isJunkContactName(p.contact_name) ? '' : p.contact_name) || (p.owner ?? '')
}

export function placeIsFeatured(p: DiscoverPlace): boolean {
  return p.highlighted === true
}

// Template cover art: the same bytes always hash the same, so a match means the creator never set one.
const DEFAULT_THUMBNAIL_HASHES = new Set([
  'bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y', // SDK7 scene template
  'bafkreiccptidkodtsbcmk7uw3afmuysu6eboocnr57dmwdg7voylyzg5hy' // empty scene template
])

function isPlaceholderImage(image?: string): boolean {
  if (!image || image.includes('/v2/map.png')) return true
  return DEFAULT_THUMBNAIL_HASHES.has(image.split('/').pop() ?? '')
}

// sdk-commands scaffolds every scene with contact name "SDK" — a default, not an identity.
function isJunkContactName(name?: string | null): boolean {
  const trimmed = name?.trim().toLowerCase()
  return !trimmed || trimmed === 'sdk'
}

/** Test deploys, auto roads, "Empty" scenes and placeholder covers — hidden like decentraland.org/places does. */
export function isHiddenPlace(p: DiscoverPlace): boolean {
  const title = (p.title ?? '').trim().toLowerCase()
  if (title.includes('interactive-text') || title === 'empty') return true
  if (!p.world && (p.title?.startsWith('Road at ') ?? false) && (p.categories?.length ?? 0) === 0) return true
  if (isPlaceholderImage(p.image) && !(p.world && placePlayers(p) > 0)) return true
  if (p.owner || !isJunkContactName(p.contact_name)) return false
  return p.world ? placePlayers(p) === 0 : true
}

export type PlaceTeleport =
  | { kind: 'world'; realm: string }
  | { kind: 'parcel'; x: number; y: number }
  | null

export function placeTeleport(p: DiscoverPlace): PlaceTeleport {
  if (p.world) {
    return p.world_name ? { kind: 'world', realm: p.world_name } : null
  }
  const coord = p.base_position ?? p.positions?.[0]
  if (!coord) return null
  const [xs, ys] = coord.split(',')
  const x = Number(xs)
  const y = Number(ys)
  if (!Number.isFinite(x) || !Number.isFinite(y) || ys === undefined) return null
  return { kind: 'parcel', x, y }
}
