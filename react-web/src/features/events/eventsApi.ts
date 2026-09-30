// Client for the public Decentraland events API (events.<base domain>/api/events).

import { useEffect, useState } from 'react'
import { BASE_DOMAIN } from '../../lib/baseDomain'

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
