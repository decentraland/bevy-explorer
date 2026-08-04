// Which super-user scene the engine boots, and whether a link is allowed to choose it.
//
// `?systemScene=` substitutes the scene that owns the UI, and that scene is trusted: permissions.rs
// short-circuits every permission check for it and it gets the whole SystemApi. So a crafted link
// on our own origin is a complete takeover of the session — no sandbox escape needed, just a click.
// Anything not on the list below gets the interstitial in features/gate/UntrustedLaunchGate.

import { PAGE_DIR } from './publicUrl'

// Our super-user bridge scene. It relays the scene-loading stream + player-ready over
// BroadcastChannel and renders no UI.
//   • PROD (built app): the EXPORTED static bundle shipped in this package, loaded from the
//     VERSIONED CDN base (BASE_URL) — NOT the page origin: the zone site mirror drops the
//     realm's extensionless files (`about`, bafk… content hashes) and 404s them, while the CDN
//     serves the full package. The engine only ever FETCHES the realm (CORS, ACAO:* on the CDN),
//     so it doesn't need to be same-origin — and version-pinning it avoids mirror skew anyway.
//   • DEV default: the LIVE preview realm from `sdk-commands start` on :8100 (started by the vite
//     plugin) — fast iteration, the scene hot-reloads. (BASE_URL is '/' in dev, so ?bundled=1
//     still resolves to this origin's /bridge-scene/static.)
export const SYSTEM_SCENE =
  import.meta.env.PROD || new URLSearchParams(location.search).has('bundled')
    ? new URL('bridge-scene/static/BevyExplorerUI', new URL(import.meta.env.BASE_URL, PAGE_DIR)).href
    : 'http://localhost:8100'

// Worlds we ship as first-party UI scenes. Accepted bare or as a full worlds-content-server url —
// the engine's ipfs layer expands `name.dcl.eth` into the latter, and boot.js reverses it for the
// address bar, so both spellings reach users.
const TRUSTED_WORLDS = ['tortilla.dcl.eth', 'sceneviewer.dcl.eth']
const WORLDS_PREFIX = 'https://worlds-content-server.decentraland.org/world/'

// A scene served from the developer's own machine. `sdk-commands start` is the whole point of the
// parameter, and a link can't make someone else's machine serve it.
const LOOPBACK = new Set(['localhost', '127.0.0.1', '[::1]'])

function normalise(value: string): string {
  return value.trim().replace(/\/+$/, '')
}

export function isTrustedSystemScene(value: string): boolean {
  const v = normalise(value)
  if (v === '') return true
  // 'none' runs NO ui scene — strictly less privilege than the default, never worth warning about.
  if (v.toLowerCase() === 'none') return true
  if (v === normalise(SYSTEM_SCENE)) return true

  // Exact matches only. A suffix/`includes` test would accept `tortilla.dcl.eth.example.com`, and a
  // prefix test on the worlds url would accept `…/world/tortilla.dcl.eth/../../elsewhere`.
  const lower = v.toLowerCase()
  if (TRUSTED_WORLDS.includes(lower)) return true
  if (lower.startsWith(WORLDS_PREFIX)) return TRUSTED_WORLDS.includes(lower.slice(WORLDS_PREFIX.length))

  try {
    if (LOOPBACK.has(new URL(v).hostname.toLowerCase())) return true
  } catch {
    // not an absolute url — a bare path can't be vouched for, so it falls through to untrusted
  }
  return false
}
