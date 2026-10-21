// Every query param the entry url may carry, in one place, so a link with anything else can be
// called out instead of silently ignored. Two sources: the engine's web param table (the params a
// LINK may set — launch, destination and the base domain; a `host` param like `editor` is an
// embedding page's to set, never a link's) and this page's own switches — the user-facing ones
// with a line of doc each so the dialog can show what IS accepted, the internal ones just known.

import { WEB_PARAMS } from '../engine/generated'
import { ANALYTICS_SESSION_PARAM, isAttributionParam } from './analytics'

export interface AcceptedParam {
  name: string
  doc: string
  /** who consumes it: the engine (via the table) or this page */
  reader: 'engine' | 'page'
}

// The page's own user-facing params — read by App / bootMode, never handed to the engine.
const PAGE_PARAMS: Record<string, string> = {
  world: 'A world by name: `world=boedo` is `realm=boedo.dcl.eth`.',
  guest: '`1` skips the sign-in screen with a guest login.',
  fps: '`1` shows the frame-rate overlay (Ctrl/Cmd+Shift+F toggles it).'
}

const WORLD_NAME = /^[a-z0-9-]+$/

/** The realm a link names: `realm`, else `world`, a name under `.dcl.eth` given bare or in full.
 *  Anything else as a world (a url, another ens) is no destination. */
export function entryRealm(q: URLSearchParams): string | null {
  const realm = q.get('realm')
  if (realm != null && realm !== '') return realm
  const world = q.get('world')?.trim().toLowerCase().replace(/\.dcl\.eth$/, '')
  return world != null && WORLD_NAME.test(world) ? `${world}.dcl.eth` : null
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
  'perm',
  'worldpw',
  ANALYTICS_SESSION_PARAM,
  // Tag Assistant's debug session, and the click ids ad platforms append to their links
  'gtm_debug',
  '_gl',
  'gclid',
  'gbraid',
  'wbraid',
  'fbclid',
  'msclkid',
  'ttclid',
  'twclid',
  'li_fat_id',
  'rdt_cid',
  // dev only (lib/systemScene.ts)
  ...(import.meta.env.PROD ? [] : ['bridgePort'])
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

/** The entry url's params that nothing reads — ignored, and worth telling the user about.
 *  `hostParams`: host-delivered params this page is acting on (the scene editor's `editor`). */
export function unrecognisedEntryParams(q: URLSearchParams, hostParams: string[] = []): string[] {
  const accepted = new Set([...acceptedEntryParams().map((p) => p.name), ...INTERNAL_PARAMS, ...hostParams])
  return [...new Set(q.keys())].filter((name) => !accepted.has(name) && !isAttributionParam(name))
}
