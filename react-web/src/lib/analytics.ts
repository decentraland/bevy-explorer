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

// the ids ad platforms append to their links, which credit a conversion to the click; Google Ads
// also adds `gad_*` keys (gad_source, gad_campaignid) and keeps adding new ones
const CLICK_ID_PARAMS = ['gclid', 'gclsrc', 'gbraid', 'wbraid', 'dclid', '_gl', 'fbclid', 'msclkid', 'ttclid', 'twclid', 'li_fat_id', 'rdt_cid']

/** A campaign's own key: any `utm_*` or `gad_*`, or an ad click id. */
export function isCampaignParam(name: string): boolean {
  return name.startsWith('utm_') || name.startsWith('gad_') || CLICK_ID_PARAMS.includes(name)
}

/** A query key read for attribution rather than called out as unknown: a campaign's, or the
 *  landing page's anonymous id. */
export function isAttributionParam(name: string): boolean {
  return name === ANON_USER_ID_PARAM || isCampaignParam(name)
}

/** Takes the attribution params, and the resumed session, off the address bar once read: a copied
 *  link or a reload must not attribute again, and the engine's url sync echoes what is left. */
export function dropAttributionParams(): void {
  const q = new URLSearchParams(location.search)
  for (const name of [...q.keys()]) if (isAttributionParam(name) || name === ANALYTICS_SESSION_PARAM) q.delete(name)
  const search = q.size > 0 ? '?' + q.toString() : ''
  if (search !== location.search) history.replaceState(null, '', location.pathname + search + location.hash)
}

/** The entry's `utm_*` params, for the sign-in page: its Segment calls read the campaign off
 *  its own url and tie it to the address on sign-in. */
export function attributionParams(): [string, string][] {
  return [...entry].filter(([name]) => name.startsWith('utm_'))
}

/** The shell's answer once its Google tag is up (shell/googleTag.ts): GA4's ids for this browser
 *  and visit, sent as `Google Tag Linked` so the warehouse joins the session to the GA4 export, the
 *  way mobile's `firebase_user_id` joins it to Firebase's. */
export function listenForGoogleTag(): void {
  window.addEventListener('message', (e: MessageEvent) => {
    if (e.source !== window.parent || e.origin !== location.origin) return
    const m = e.data as { type?: unknown; client_id?: unknown; session_id?: unknown } | null
    if (m?.type !== 'bevy-shell:google-tag' || typeof m.client_id !== 'string' || !m.client_id) return
    track('Google Tag Linked', { ga_client_id: m.client_id, ga_session_id: typeof m.session_id === 'string' ? m.session_id : '' })
  })
}

/** What a page load reports before any screen shows, as mobile's app start does. The return from
 *  the sign-in redirect continues the session instead. */
export function bootAnalytics(): void {
  if (resumedSession != null) return
  track('App Opened', {
    start_kind: 'cold',
    // a destination on the link (lib/entryParams.ts entryRealm reads world as a realm)
    trigger: entry.has('realm') || entry.has('world') || entry.has('position') ? 'deeplink' : 'icon',
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
