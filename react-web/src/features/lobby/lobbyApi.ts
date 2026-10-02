// The lobby's data: the curated destinations, the live events and busiest places, the
// landing place for the start destination, and the places visited most recently.

import { serviceUrl } from '../../lib/baseDomain'
import { EVENTS_API, type DclEvent } from '../events/eventsApi'
import { DEFAULT_PLACES_ARGS, fetchLiveWorlds, fetchPlaces, placePlayers, type DiscoverPlace } from '../places/placesApi'

const PLACES_API = `${serviceUrl('places')}/api`
const RECENTS_KEY = 'lobby.recentPlaces'
const RECENTS_MAX = 20
export const RECENTS_SHOWN = 3

async function placesData(url: string, init?: RequestInit): Promise<DiscoverPlace[]> {
  const res = await fetch(url, init)
  if (!res.ok) throw new Error(`places API failed: ${res.status}`)
  const json = (await res.json()) as { ok?: boolean; data?: DiscoverPlace[] }
  if (json.ok === false || !Array.isArray(json.data)) throw new Error('places API returned not-ok')
  return json.data
}

export function fetchHighlighted(): Promise<DiscoverPlace[]> {
  return placesData(`${PLACES_API}/destinations?with_realms_detail=true&only_highlighted=true`)
}

/** The place covering a parcel (null when the parcel is empty). */
export async function fetchPlaceAt(x: number, y: number): Promise<DiscoverPlace | null> {
  const data = await placesData(`${PLACES_API}/places?with_realms_detail=true&positions=${x},${y}`)
  return data[0] ?? null
}

export async function fetchWorld(name: string): Promise<DiscoverPlace | null> {
  const data = await placesData(`${PLACES_API}/worlds?names=${encodeURIComponent(name)}`)
  return data[0] ?? null
}

export type LiveEvent = DclEvent & { connected_addresses?: string[] }

/** People in a live event right now. */
export function eventPeople(e: LiveEvent): number {
  return e.connected_addresses?.length ?? 0
}

export interface LobbyEvents {
  live: LiveEvent[]
}

export async function fetchLobbyEvents(signal?: AbortSignal): Promise<LobbyEvents> {
  const res = await fetch(`${EVENTS_API}?with_connected_users=true`, { signal })
  if (!res.ok) throw new Error(`Events service returned ${res.status}`)
  const body = (await res.json()) as { ok?: boolean; data?: LiveEvent[] }
  if (body.ok === false || !Array.isArray(body.data)) throw new Error('Events service returned an unexpected response')
  return { live: body.data.filter((e) => e.live).sort((a, b) => eventPeople(b) - eventPeople(a)) }
}

export const LIVE_PLACES_MAX = 10

/** Places and worlds with people in them right now, busiest first. */
export async function fetchLivePlaces(): Promise<DiscoverPlace[]> {
  const [places, worlds] = await Promise.all([
    fetchPlaces(DEFAULT_PLACES_ARGS).then((r) => r.data).catch(() => []),
    fetchLiveWorlds().catch(() => [])
  ])
  const ids = new Set(places.map((p) => p.id))
  return [...places, ...worlds.filter((w) => !ids.has(w.id))]
    .filter((p) => placePlayers(p) > 0)
    .sort((a, b) => placePlayers(b) - placePlayers(a))
    .slice(0, LIVE_PLACES_MAX)
}


function readRecents(): string[] {
  try {
    const raw = JSON.parse(localStorage.getItem(RECENTS_KEY) ?? '[]') as unknown
    return Array.isArray(raw) ? raw.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

/** Remember a visited place, most recent first. */
export function rememberPlace(id: string): void {
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
