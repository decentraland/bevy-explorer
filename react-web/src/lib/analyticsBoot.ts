// What a page load reports before any screen shows, mirroring mobile's app start: "App Opened",
// "Install Attribution" (once per device), the START screen. Runs once on the ENTRY url's query.
// The auth redirect reloads the page but is one sign-in: the returning load continues the session.

import { analytics } from './analytics'
import { getPref, PREF, setPref } from './prefs'

const LAST_SEEN_INTERVAL_MS = 30_000
// mobile's warm-open floor, and how long the auth hop may take before the return is a new session
const WARM_OPEN_FLOOR_S = 30
const AUTH_RETURN_MAX_S = 30 * 60
const UTM_KEYS = ['utm_source', 'utm_medium', 'utm_campaign', 'utm_content', 'utm_term'] as const

interface SessionMark {
  session: string
  at: number
}

/** A query key the page reads for attribution: any `utm_*`, or the campaign token `c`. */
export function isAttributionParam(name: string): boolean {
  return name === 'c' || name.startsWith('utm_')
}

/** Right before the auth redirect: the page load that returns continues this session. */
export function markAuthRedirect(now = Date.now()): void {
  setPref(PREF.analyticsRedirect, JSON.stringify({ session: analytics.sessionId(), at: now }))
}

export function bootAnalytics(entrySearch: string, now = Date.now()): void {
  const resumed = takeAuthRedirect(now)
  if (resumed != null) {
    analytics.resumeSession(resumed.session)
    const since = secondsSince(readMark(PREF.analyticsLastSeen), now)
    if (since >= WARM_OPEN_FLOOR_S) {
      analytics.track('App Opened', { start_kind: 'warm', trigger: 'icon', prev_session_id: resumed.session, seconds_since_last_seen: since })
    }
  } else {
    const q = new URLSearchParams(entrySearch)
    const last = readMark(PREF.analyticsLastSeen)
    analytics.track('App Opened', {
      start_kind: 'cold',
      trigger: q.has('realm') || q.has('position') ? 'deeplink' : 'icon',
      prev_session_id: last?.session ?? '',
      seconds_since_last_seen: secondsSince(last, now)
    })
    trackInstallAttribution(q, entrySearch, now)
    analytics.screenViewed('START', { startup_time_ms: Math.round(performance.now()), platform: 'web' })
  }

  markSeen()
  setInterval(markSeen, LAST_SEEN_INTERVAL_MS)
  window.addEventListener('pagehide', markSeen)
}

// once per device, like mobile's once per install
function trackInstallAttribution(q: URLSearchParams, entrySearch: string, now: number): void {
  if (!q.has('c') && !UTM_KEYS.some((k) => q.has(k))) return
  if (getPref(PREF.analyticsAttributed)) return
  const referrer = entrySearch.replace(/^\?/, '')
  const seconds = Math.floor(now / 1000)
  const utm: Record<string, string> = {}
  for (const key of UTM_KEYS) {
    const value = q.get(key)
    if (value != null) utm[key] = value
  }
  const token = q.get('c')
  analytics.track('Install Attribution', {
    referrer,
    ...utm,
    ...(token == null ? {} : { campaign_token: token }),
    click_timestamp: seconds,
    install_timestamp: seconds,
    google_play_instant: false,
    attribution_source: 'web_url',
    referrer_settled: true
  })
  setPref(PREF.analyticsAttributed, referrer)
}

// consumed whether or not it still counts, so it never attributes a later load
function takeAuthRedirect(now: number): SessionMark | null {
  const mark = readMark(PREF.analyticsRedirect)
  if (mark == null) return null
  setPref(PREF.analyticsRedirect, '')
  return now - mark.at <= AUTH_RETURN_MAX_S * 1000 ? mark : null
}

function readMark(key: string): SessionMark | null {
  try {
    const parsed: unknown = JSON.parse(getPref(key) || 'null')
    if (parsed == null || typeof parsed !== 'object') return null
    const { session, at } = parsed as { session?: unknown; at?: unknown }
    return typeof session === 'string' && session !== '' && typeof at === 'number' ? { session, at } : null
  } catch {
    return null
  }
}

// -1 when unknown or the clock moved backwards, as mobile reads it
function secondsSince(mark: SessionMark | null, now: number): number {
  return mark != null && now >= mark.at ? Math.floor((now - mark.at) / 1000) : -1
}

function markSeen(): void {
  setPref(PREF.analyticsLastSeen, JSON.stringify({ session: analytics.sessionId(), at: Date.now() }))
}
