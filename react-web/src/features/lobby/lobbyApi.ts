// The lobby's data: the curated destinations, one events fetch split into live and upcoming, the
// landing place for the start destination, and the places visited most recently.

import { serviceUrl } from '../../lib/baseDomain'
import { EVENTS_API, type DclEvent } from '../events/eventsApi'
import type { DiscoverPlace } from '../places/placesApi'

const PLACES_API = `${serviceUrl('places')}/api`
const RECENTS_KEY = 'lobby.recentPlaces'
const RECENTS_MAX = 20
export const RECENTS_SHOWN = 3
export const UPCOMING_MAX = 10

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

export interface LobbyEvents {
  live: DclEvent[]
  upcoming: DclEvent[]
}

export async function fetchLobbyEvents(signal?: AbortSignal): Promise<LobbyEvents> {
  const res = await fetch(`${EVENTS_API}?with_connected_users=true`, { signal })
  if (!res.ok) throw new Error(`Events service returned ${res.status}`)
  const body = (await res.json()) as { ok?: boolean; data?: Array<DclEvent & { next_start_at?: string }> }
  if (body.ok === false || !Array.isArray(body.data)) throw new Error('Events service returned an unexpected response')
  const startOf = (e: DclEvent & { next_start_at?: string }): number => Date.parse(e.next_start_at ?? e.start_at)
  return {
    live: body.data.filter((e) => e.live),
    upcoming: body.data
      .filter((e) => !e.live)
      .sort((a, b) => startOf(a) - startOf(b))
      .slice(0, UPCOMING_MAX)
  }
}

/** "Starting now" / "In 5 min" / "In 2 hours" / "In 3 days". */
export function startsIn(startAt: string, now = Date.now()): string {
  const minutes = Math.round((Date.parse(startAt) - now) / 60_000)
  if (minutes <= 0) return 'Starting now'
  if (minutes < 60) return `In ${minutes} min`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `In ${hours} hour${hours === 1 ? '' : 's'}`
  const days = Math.round(hours / 24)
  return `In ${days} day${days === 1 ? '' : 's'}`
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
