// Every query param the entry url may carry, in one place, so a link with anything else can be
// called out instead of silently ignored. Two sources: the engine's web param table (the params a
// LINK may set — launch, destination and the base domain; a `host` param like `editor` is an
// embedding page's to set, never a link's) and this page's own switches — the user-facing ones
// with a line of doc each so the dialog can show what IS accepted, the internal ones just known.

import { WEB_PARAMS } from '../engine/generated'

export interface AcceptedParam {
  name: string
  doc: string
  /** who consumes it: the engine (via the table) or this page */
  reader: 'engine' | 'page'
}

// The page's own user-facing params — read by App / bootMode, never handed to the engine.
const PAGE_PARAMS: Record<string, string> = {
  guest: '`1` skips the sign-in screen with a guest login.',
  fps: '`1` shows the frame-rate overlay (Ctrl/Cmd+Shift+F toggles it).'
}

// Also read by this page, but not a user's to set: forced by the host (native, hud), or debug /
// test switches (mock, showcase, the gate overrides, the mock-bridge fixtures). Accepted without a
// warning, not advertised.
const INTERNAL_PARAMS = [
  'native',
  'hud',
  'mock',
  'showcase',
  'gate',
  'nogate',
  'bundled',
  'simerror',
  'simhover',
  'previousLogin',
  'perm'
]

/** What the dialog advertises: every link-settable engine param, then the page's own. */
export function acceptedEntryParams(): AcceptedParam[] {
  const engine = WEB_PARAMS.filter((p) => p.delivery !== 'host').map((p) => ({
    name: p.name,
    doc: p.doc,
    reader: 'engine' as const
  }))
  const page = Object.entries(PAGE_PARAMS).map(([name, doc]) => ({ name, doc, reader: 'page' as const }))
  return [...engine, ...page]
}

/** The entry url's params that nothing reads — ignored, and worth telling the user about. */
export function unrecognisedEntryParams(q: URLSearchParams): string[] {
  const accepted = new Set([...acceptedEntryParams().map((p) => p.name), ...INTERNAL_PARAMS])
  return [...new Set(q.keys())].filter((name) => !accepted.has(name))
}
