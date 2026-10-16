// The page's Segment events (the screens and clicks mobile reports), stamped at call time and
// handed to the engine's `/track` command (crates/analytics/README.md). The queue lives in prefs
// so the sign-in screen's events survive the auth redirect.

import { getPref, PREF, setPref } from './prefs'

export type PageEvent = 'Screen Viewed' | 'Click Button' | 'Install Attribution' | 'App Opened' | 'Guest Wallet Creation'

export interface QueuedEvent {
  event: PageEvent
  properties: Record<string, unknown>
  created_at: string
  message_id: string
  session_id: string
}

export interface QueueStore {
  load(): string | null
  save(json: string): void
}

export type EngineConsole = (line: string) => Promise<string>

export interface Analytics {
  track(event: PageEvent, properties: Record<string, unknown>): void
  screenViewed(screenName: string, extra?: Record<string, unknown>): void
  clickButton(buttonText: string, screenName: string, extra?: Record<string, unknown>): void
  sessionId(): string
  /** Continue an earlier page load's session (the return from the auth redirect); call before the first event. */
  resumeSession(id: string): void
  /** Where `/track` goes. Unattached (mock, tests), events only accumulate. */
  attachEngine(run: EngineConsole): void
  /** Send what is queued now; a failure keeps the events for the next track()/flush(). */
  flush(): Promise<void>
  pending(): readonly QueuedEvent[]
}

// Newest kept when nothing drains them (the engine never came up, a gated browser).
export const QUEUE_CAP = 200
// The engine's /track takes at most this many per command.
export const BATCH_SIZE = 50

export function createAnalytics(store: QueueStore, session: string = crypto.randomUUID(), debug = false): Analytics {
  let queue: QueuedEvent[] | null = null
  let run: EngineConsole | null = null
  let inflight: Promise<void> | null = null
  let again = false
  let saving = false

  const items = (): QueuedEvent[] => {
    if (queue == null) queue = restore(store)
    return queue
  }
  // coalesced: a burst of events is one write (on native each write rewrites the prefs file)
  const persist = (): void => {
    if (saving) return
    saving = true
    queueMicrotask(() => {
      saving = false
      try {
        store.save(JSON.stringify(items()))
      } catch {}
    })
  }

  const drain = (): Promise<void> => {
    if (run == null) return Promise.resolve()
    if (inflight != null) {
      again = true
      return inflight
    }
    const send = run
    inflight = (async () => {
      try {
        while (items().length > 0) {
          const batch = items().slice(0, BATCH_SIZE)
          const reply = await send(`/track ${encodeBatch(batch)}`)
          if (debug && !reply.startsWith('ok')) console.debug('[analytics] engine', reply)
          const sent = new Set(batch.map((e) => e.message_id))
          queue = items().filter((e) => !sent.has(e.message_id))
          persist()
        }
      } catch (e) {
        if (debug) console.debug('[analytics] send failed, kept', items().length, e)
      } finally {
        inflight = null
      }
      if (again) {
        again = false
        await drain()
      }
    })()
    return inflight
  }

  const track = (event: PageEvent, properties: Record<string, unknown>): void => {
    const queued: QueuedEvent = {
      event,
      properties,
      created_at: new Date().toISOString(),
      message_id: crypto.randomUUID(),
      session_id: session
    }
    if (debug) console.debug('[analytics]', event, properties)
    const list = items()
    list.push(queued)
    if (list.length > QUEUE_CAP) list.splice(0, list.length - QUEUE_CAP)
    persist()
    void drain()
  }

  return {
    track,
    screenViewed: (screenName, extra) => track('Screen Viewed', { screen_name: screenName, ...extraProperties(extra) }),
    clickButton: (buttonText, screenName, extra) =>
      track('Click Button', { button_text: buttonText, screen_name: screenName, ...extraProperties(extra) }),
    sessionId: () => session,
    resumeSession: (id) => {
      session = id
    },
    attachEngine: (engine) => {
      run = engine
      void drain()
    },
    flush: drain,
    pending: () => items()
  }
}

// Mobile sends `extra_properties` as a JSON string, and leaves it out when there is none.
function extraProperties(extra: Record<string, unknown> | undefined): { extra_properties?: string } {
  return extra == null ? {} : { extra_properties: JSON.stringify(extra) }
}

function restore(store: QueueStore): QueuedEvent[] {
  try {
    const parsed: unknown = JSON.parse(store.load() ?? '[]')
    return Array.isArray(parsed) ? (parsed as QueuedEvent[]) : []
  } catch {
    return []
  }
}

// The console splits a command line on whitespace, so the batch travels as one base64 token.
// btoa takes a binary string, so the UTF-8 bytes go through one char each.
export function encodeBatch(batch: readonly QueuedEvent[]): string {
  const bytes = new TextEncoder().encode(JSON.stringify(batch))
  let binary = ''
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(binary)
}

const prefsStore: QueueStore = {
  load: () => getPref(PREF.analyticsQueue),
  save: (json) => setPref(PREF.analyticsQueue, json)
}

// Read before the engine's url sync rewrites the query.
const debugOn = new URLSearchParams(location.search).get('analyticsDebug') === '1'

export const analytics: Analytics = createAnalytics(prefsStore, crypto.randomUUID(), debugOn)

export const track = analytics.track
export const screenViewed = analytics.screenViewed
export const clickButton = analytics.clickButton
export const sessionId = analytics.sessionId
export const attachEngine = analytics.attachEngine
export const flush = analytics.flush
