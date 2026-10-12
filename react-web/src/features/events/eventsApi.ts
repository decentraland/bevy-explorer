// Client for the public Decentraland events API (events.<base domain>/api/events).

import { useEffect, useState } from 'react'
import { BASE_DOMAIN, serviceUrl } from '../../lib/baseDomain'

export const EVENTS_API = `https://events.${BASE_DOMAIN}/api/events`
// Unity refreshes the sidebar's live counter every 3 minutes (SidebarController.FillLiveEventsAsync).
export const LIVE_COUNT_REFRESH_MS = 3 * 60 * 1000

export type EventsList = 'live' | 'upcoming'

export interface DclEvent {
  id: string
  name: string
  image?: string | null
  x: number
  y: number
  server?: string | null
  world?: boolean
  live?: boolean
  start_at: string
  finish_at?: string
  total_attendees?: number
  user_name?: string | null
}

export async function fetchEvents(list: EventsList, signal?: AbortSignal): Promise<DclEvent[]> {
  const res = await fetch(`${EVENTS_API}?list=${list}`, { signal })
  if (!res.ok) throw new Error(`Events service returned ${res.status}`)
  const body = (await res.json()) as { ok?: boolean; data?: DclEvent[] }
  if (body.ok === false || !Array.isArray(body.data)) throw new Error('Events service returned an unexpected response')
  return body.data
}

// People in the scene right now, keyed the way an event points at its venue. total_attendees is
// RSVPs, not presence; decentraland.org/events reads live counts from hot-scenes the same way.
export interface LiveHeadcounts {
  /** Every parcel of a populated Genesis City scene → that scene's head count. */
  parcels: Map<string, number>
  /** World name (lowercase) → head count. */
  worlds: Map<string, number>
}

type HotScene = { parcels?: [number, number][]; usersTotalCount?: number }
type WorldsLiveData = { data?: { perWorld?: { worldName: string; users: number }[] | Record<string, number> } }

// Presence is optional: a failed, non-JSON or malformed reply means "no counts", never an error.
function json(url: string, signal?: AbortSignal): Promise<unknown> {
  return fetch(url, { signal })
    .then((res) => (res.ok ? (res.json() as Promise<unknown>) : null))
    .catch(() => null)
}

export async function fetchLiveHeadcounts(signal?: AbortSignal): Promise<LiveHeadcounts> {
  const [scenes, live] = await Promise.all([
    json(`${serviceUrl('realmProvider')}/hot-scenes`, signal),
    json(`${serviceUrl('worldsServer')}/live-data`, signal)
  ])
  const parcels = new Map<string, number>()
  for (const scene of Array.isArray(scenes) ? (scenes as HotScene[]) : []) {
    const users = typeof scene?.usersTotalCount === 'number' ? scene.usersTotalCount : 0
    for (const parcel of Array.isArray(scene?.parcels) ? scene.parcels : []) {
      if (Array.isArray(parcel)) parcels.set(`${parcel[0]},${parcel[1]}`, users)
    }
  }
  const perWorld = (live as WorldsLiveData | null)?.data?.perWorld
  const entries: [unknown, unknown][] = Array.isArray(perWorld)
    ? perWorld.map((w) => [w?.worldName, w?.users])
    : perWorld && typeof perWorld === 'object'
      ? Object.entries(perWorld)
      : []
  const worlds = new Map<string, number>()
  for (const [name, users] of entries) {
    if (typeof name === 'string' && typeof users === 'number') worlds.set(name.toLowerCase(), users)
  }
  return { parcels, worlds }
}

export function eventPeopleNow(e: DclEvent, counts: LiveHeadcounts): number {
  if (e.world) return counts.worlds.get((e.server ?? '').toLowerCase()) ?? 0
  return counts.parcels.get(`${e.x},${e.y}`) ?? 0
}

export type EventDestination = { kind: 'world'; realm: string } | { kind: 'parcel'; x: number; y: number }

export function eventDestination(e: DclEvent): EventDestination | null {
  if (e.world) return e.server ? { kind: 'world', realm: e.server } : null
  return Number.isFinite(e.x) && Number.isFinite(e.y) ? { kind: 'parcel', x: e.x, y: e.y } : null
}

export function eventLocation(e: DclEvent): string {
  return e.world ? (e.server ?? '') : `${e.x},${e.y}`
}

/** Live events right now; 0 while loading or when the service is unreachable (as Unity). */
export function useLiveEventCount(): number {
  const [count, setCount] = useState(0)
  useEffect(() => {
    const ctrl = new AbortController()
    const load = (): void => {
      fetchEvents('live', ctrl.signal).then(
        (list) => setCount(list.length),
        () => !ctrl.signal.aborted && setCount(0)
      )
    }
    load()
    const id = setInterval(load, LIVE_COUNT_REFRESH_MS)
    return () => {
      ctrl.abort()
      clearInterval(id)
    }
  }, [])
  return count
}
