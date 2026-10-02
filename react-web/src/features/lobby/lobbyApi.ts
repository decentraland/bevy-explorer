// The lobby's data: the curated destinations, the live events and busiest places, the landing
// place for the home destination, and the places visited most recently.

import { serviceUrl } from '../../lib/baseDomain'
import { EVENTS_API, type DclEvent } from '../events/eventsApi'
import { DEFAULT_PLACES_ARGS, fetchLiveWorlds, fetchPlaces, placePlayers, type DiscoverPlace } from '../places/placesApi'

const PLACES_API = `${serviceUrl('places')}/api`
const RECENTS_KEY = 'lobby.recentPlaces'
const RECENTS_MAX = 20
const RECENTS_SHOWN = 3
const LIVE_PLACES_MAX = 10
const CACHE_TTL = 60_000

// Reopening the lobby (or walking between scenes) shouldn't refetch what it just had.
const cache = new Map<string, { at: number; promise: Promise<unknown> }>()
function cached<T>(key: string, load: () => Promise<T>): Promise<T> {
  const hit = cache.get(key)
  if (hit && Date.now() - hit.at < CACHE_TTL) return hit.promise as Promise<T>
  const promise = load()
  cache.set(key, { at: Date.now(), promise })
  promise.catch(() => {
    if (cache.get(key)?.promise === promise) cache.delete(key)
  })
  return promise
}

async function placesData(url: string, init?: RequestInit): Promise<DiscoverPlace[]> {
  const res = await fetch(url, init)
  if (!res.ok) throw new Error(`places API failed: ${res.status}`)
  const json = (await res.json()) as { ok?: boolean; data?: DiscoverPlace[] }
  if (json.ok === false || !Array.isArray(json.data)) throw new Error('places API returned not-ok')
  return json.data
}

export function fetchHighlighted(): Promise<DiscoverPlace[]> {
  return cached('highlighted', () => placesData(`${PLACES_API}/destinations?with_realms_detail=true&only_highlighted=true`))
}

/** The place covering a parcel (null when the parcel is empty). */
export function fetchPlaceAt(x: number, y: number): Promise<DiscoverPlace | null> {
  return cached(`place:${x},${y}`, async () => (await placesData(`${PLACES_API}/places?with_realms_detail=true&positions=${x},${y}`))[0] ?? null)
}

export function fetchWorld(name: string): Promise<DiscoverPlace | null> {
  return cached(`world:${name}`, async () => (await placesData(`${PLACES_API}/worlds?names=${encodeURIComponent(name)}`))[0] ?? null)
}

export type LiveEvent = DclEvent & { connected_addresses?: string[] }

/** People in a live event right now. */
export function eventPeople(e: LiveEvent): number {
  return e.connected_addresses?.length ?? 0
}

/** The live events, busiest first. */
export function fetchLiveEvents(): Promise<LiveEvent[]> {
  return cached('liveEvents', async () => {
    const res = await fetch(`${EVENTS_API}?with_connected_users=true`)
    if (!res.ok) throw new Error(`Events service returned ${res.status}`)
    const body = (await res.json()) as { ok?: boolean; data?: LiveEvent[] }
    if (body.ok === false || !Array.isArray(body.data)) throw new Error('Events service returned an unexpected response')
    return body.data.filter((e) => e.live).sort((a, b) => eventPeople(b) - eventPeople(a))
  })
}

/** Places and worlds with people in them right now, busiest first. */
export function fetchLivePlaces(): Promise<DiscoverPlace[]> {
  return cached('livePlaces', async () => {
    const [places, worlds] = await Promise.all([
      fetchPlaces(DEFAULT_PLACES_ARGS).then((r) => r.data).catch(() => []),
      fetchLiveWorlds().catch(() => [])
    ])
    const ids = new Set(places.map((p) => p.id))
    return [...places, ...worlds.filter((w) => !ids.has(w.id))]
      .filter((p) => placePlayers(p) > 0)
      .sort((a, b) => placePlayers(b) - placePlayers(a))
      .slice(0, LIVE_PLACES_MAX)
  })
}

/** The landing place for the home destination: a World by name, else the place at its parcel. */
export function fetchHomePlace(home: { realm: string | null; parcel: string }): Promise<DiscoverPlace | null> {
  if (home.realm != null && home.realm.includes('.')) return fetchWorld(home.realm)
  const [x, y] = home.parcel.split(',').map(Number)
  return Number.isFinite(x) && Number.isFinite(y) ? fetchPlaceAt(x, y) : Promise.resolve(null)
}

function readRecents(): string[] {
  try {
    const raw = JSON.parse(localStorage.getItem(RECENTS_KEY) ?? '[]') as unknown
    return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

function rememberPlace(id: string): void {
  const next = [id, ...readRecents().filter((r) => r !== id)].slice(0, RECENTS_MAX)
  try {
    localStorage.setItem(RECENTS_KEY, JSON.stringify(next))
  } catch {
    /* storage unavailable: recents just stay empty */
  }
}

/** Record the place the player is standing in: Genesis City by parcel, a World by its name. */
export async function recordVisit(visit: { realm: string; genesis: boolean; parcel: { x: number; y: number } }): Promise<void> {
  const place = visit.genesis ? await fetchPlaceAt(visit.parcel.x, visit.parcel.y) : await fetchWorld(visit.realm)
  if (place != null) rememberPlace(place.id)
}

/** The recently visited places, in visit order. */
export async function fetchRecents(): Promise<DiscoverPlace[]> {
  const ids = readRecents()
  if (ids.length === 0) return []
  const data = await placesData(`${PLACES_API}/destinations`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(ids)
  })
  const byId = new Map(data.map((p) => [p.id, p]))
  return ids.flatMap((id) => byId.get(id) ?? []).slice(0, RECENTS_SHOWN)
}
