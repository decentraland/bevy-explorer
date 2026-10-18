// The page's Segment events: the screens and clicks mobile reports, with the same names and
// property shapes, so one dashboard reads both clients. Nothing is sent from here: each event is
// stamped when it happens and handed to the engine's `/track` command once the engine is up, and
// the engine posts it with its own events through the first-party proxy, adding the common fields
// (the session id, the signed-in address, the realm) as it does for its own. Until then the queue
// waits in prefs, so the sign-in screen's events survive the auth redirect.

import { getPref, PREF, setPref } from './prefs'

export interface QueuedEvent {
  event: string
  properties: Record<string, unknown>
  timestamp: string
}

export type EngineConsole = (line: string) => Promise<string>

// The app's query params the shell sets: the session to continue after the sign-in redirect,
// and the landing page's Segment anonymous id (the shell reads the cookie; the app can't).
export const ANALYTICS_SESSION_PARAM = 'analyticsSession'
export const ANON_USER_ID_PARAM = 'anon_user_id'

// Newest kept when the engine never comes up to drain them.
const QUEUE_CAP = 200

// Read before the engine's url sync rewrites the query.
const entry = new URLSearchParams(location.search)
const resumedSession = entry.get(ANALYTICS_SESSION_PARAM)
const session = resumedSession ?? crypto.randomUUID()
const campaignAnonUserId = entry.get(ANON_USER_ID_PARAM)

let queue: QueuedEvent[] | null = null
let run: EngineConsole | null = null

const items = (): QueuedEvent[] => (queue ??= restore())
const persist = (): void => {
  try {
    setPref(PREF.analyticsQueue, JSON.stringify(items()))
  } catch {}
}

function restore(): QueuedEvent[] {
  try {
    const parsed: unknown = JSON.parse(getPref(PREF.analyticsQueue) || '[]')
    return Array.isArray(parsed) ? parsed.filter(isQueuedEvent) : []
  } catch {
    return []
  }
}

function isQueuedEvent(value: unknown): value is QueuedEvent {
  const e = value as Partial<QueuedEvent> | null
  return e != null && typeof e.event === 'string' && typeof e.timestamp === 'string' && e.properties != null && typeof e.properties === 'object'
}

// Everything pending goes in one command; the engine batches the Segment posts itself. A send
// the console rejected goes back in front for the next one.
async function flush(): Promise<void> {
  if (run == null || items().length === 0) return
  const batch = items().splice(0)
  persist()
  try {
    await run(`/track ${encodeBatch(batch)}`)
  } catch (e) {
    items().unshift(...batch)
    persist()
    console.debug('[analytics] send failed, kept', items().length, e)
  }
}

export function track(event: string, properties: Record<string, unknown> = {}): void {
  const list = items()
  list.push({ event, properties, timestamp: new Date().toISOString() })
  if (list.length > QUEUE_CAP) list.splice(0, list.length - QUEUE_CAP)
  persist()
  void flush()
}

export function screenViewed(screenName: string, extra?: Record<string, unknown>): void {
  track('Screen Viewed', { screen_name: screenName, ...extraProperties(extra) })
}

export function clickButton(buttonText: string, screenName: string, extra?: Record<string, unknown>): void {
  track('Click Button', { button_text: buttonText, screen_name: screenName, ...extraProperties(extra) })
}

// Mobile sends `extra_properties` as a JSON string, and leaves it out when there is none.
function extraProperties(extra: Record<string, unknown> | undefined): { extra_properties?: string } {
  return extra == null ? {} : { extra_properties: JSON.stringify(extra) }
}

/** Where `/track` goes, from the engine's launch on (its console answers once it is up).
 *  Unattached (mock, tests), events queue. */
export function attachEngine(engine: EngineConsole): void {
  run = engine
  void flush()
}

export function pending(): readonly QueuedEvent[] {
  return items()
}

/** What the engine gets at launch, so its events carry the same session and campaign id. */
export function launchAnalytics(): { sessionId: string; campaignAnonUserId?: string } {
  return { sessionId: session, ...(campaignAnonUserId == null ? {} : { campaignAnonUserId }) }
}

export function sessionId(): string {
  return session
}

/** A query key read for attribution rather than called out as unknown: any `utm_*`, or the
 *  landing page's anonymous id. */
export function isAttributionParam(name: string): boolean {
  return name === ANON_USER_ID_PARAM || name.startsWith('utm_')
}

/** What a page load reports before any screen shows, as mobile's app start does. The return from
 *  the sign-in redirect continues the session instead. */
export function bootAnalytics(): void {
  if (resumedSession != null) return
  track('App Opened', {
    start_kind: 'cold',
    trigger: entry.has('realm') || entry.has('position') ? 'deeplink' : 'icon',
    prev_session_id: '',
    seconds_since_last_seen: -1
  })
  screenViewed('START', { startup_time_ms: Math.round(performance.now()), platform: 'web' })
}

// The console splits a command line on whitespace, so the batch travels as one base64 token.
// btoa takes a binary string, so the UTF-8 bytes go through one char each.
export function encodeBatch(batch: readonly QueuedEvent[]): string {
  const bytes = new TextEncoder().encode(JSON.stringify(batch))
  let binary = ''
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(binary)
}
