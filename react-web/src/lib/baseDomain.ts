// The base domain every HUD backend host is composed from:
//   1. the ?baseDomain= entry param (set by hand on web, injected by the native shell from
//      --base-domain; non-decentraland values are stopped by the UntrustedLaunchGate)
//   2. else derived from the hosting origin — the staging deployment at decentraland.zone/bevy-web
//      keys to zone backends, prod to org
//   3. else decentraland.org (localhost dev, the native CEF page, everything else)
// Captured once from the entry URL. This module is the HUD's single source; the ENGINE takes the
// resolved value as an ordinary engine_run option (EngineHost puts `baseDomain` in the boot
// config, the same route as every other launch param — the wasm never reads it from the page),
// and echoes it back into the url like the rest (boot.js drops it when it is the derived default).
//
// On top of it, per-service urls (serviceUrl): a ?<service>= entry param — a FULL base url, the
// web form of the native --<service> flags — else composed from the base domain by the
// convention in the engine's service table (crates/system_api_types/src/services.rs, generated
// into engine/generated/serviceTable.ts). The explicit overrides go to the engine the same way
// (SERVICE_OVERRIDES, spread into the boot config); window.__serviceUrl(name) is for the engine
// loader (deploy/web/engine/boot.js), which composes the default realm / worlds prefix for its
// url sync exactly as the HUD does.

import { SERVICES, type ServiceDef } from '../engine/generated'

// Decentraland's own deployments — the hosting origins we derive from, and the only domains a
// LINK may point the session at without the UntrustedLaunchGate (lib/launchGate.ts).
const TRUSTED_BASE_DOMAINS = ['decentraland.org', 'decentraland.zone']

/** The apex deployment domain a hosting origin implies, or null for unrecognised hosts. */
export function hostBaseDomain(hostname: string): string | null {
  for (const apex of TRUSTED_BASE_DOMAINS) {
    if (hostname === apex || hostname.endsWith(`.${apex}`)) return apex
  }
  return null
}

/**
 * The ?baseDomain= value the engine will accept (crates/common/src/base_domain.rs `set`):
 * lowercased, bare ascii labels, at least one dot. Anything else is null so the HUD and the
 * engine fall back to the same derived default instead of splitting across two domains.
 */
export function normaliseBaseDomain(raw: string | null): string | null {
  const d = raw?.trim().toLowerCase() ?? ''
  return /^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(d) ? d : null
}

/** The domain a url without ?baseDomain= means here: derived from the hosting origin, else org. */
export const DEFAULT_BASE_DOMAIN = hostBaseDomain(window.location.hostname) ?? 'decentraland.org'

export const BASE_DOMAIN =
  normaliseBaseDomain(new URLSearchParams(window.location.search).get('baseDomain')) ?? DEFAULT_BASE_DOMAIN

declare global {
  interface Window {
    __defaultBaseDomain?: () => string
    __defaultRealm?: () => string
    __serviceUrl?: (name: string) => string
  }
}
window.__defaultBaseDomain = () => DEFAULT_BASE_DOMAIN

/** A service table row by web param name. Throws so a renamed service fails at module load. */
function service(name: string): ServiceDef {
  const s = SERVICES.find((s) => s.name === name)
  if (s == null) throw new Error(`serviceUrl: '${name}' is not in the engine's service table`)
  return s
}

/**
 * The ?<service>= value the engine will accept (crates/common/src/base_domain.rs
 * `set_services`): a full http(s) — ws(s) for the websocket services — base url with no query or
 * fragment, trailing slash dropped. Anything else is null so the HUD and the engine fall back to
 * the same composed default rather than splitting. Recomposed from scheme, host and path rather
 * than serialised: a bare trailing `?` or `#` is empty to `search`/`hash` but a query/fragment to
 * the engine's parser, which would refuse it and fail the launch.
 */
export function normaliseServiceUrl(s: ServiceDef, raw: string | null): string | null {
  if (raw == null) return null
  try {
    const u = new URL(raw.trim())
    const ok = s.scheme === 'wss' ? /^wss?:$/ : /^https?:$/
    if (!ok.test(u.protocol) || u.search !== '' || u.hash !== '') return null
    return `${u.protocol}//${u.host}${u.pathname}`.replace(/\/+$/, '')
  } catch {
    return null
  }
}

/** The explicit overrides in the ENTRY url, by service name — also the engine's, via the boot config. */
export const SERVICE_OVERRIDES: Readonly<Record<string, string>> = {}
{
  const q = new URLSearchParams(window.location.search)
  for (const s of SERVICES) {
    const url = normaliseServiceUrl(s, q.get(s.name))
    if (url != null) (SERVICE_OVERRIDES as Record<string, string>)[s.name] = url
  }
}

/**
 * A service's base url by its table name: the entry url's override, else composed from the base
 * domain, e.g. serviceUrl('places') → "https://places.decentraland.org".
 */
export function serviceUrl(name: string): string {
  const s = service(name)
  const host = s.sub === '' ? BASE_DOMAIN : `${s.sub}.${BASE_DOMAIN}`
  return SERVICE_OVERRIDES[name] ?? `${s.scheme}://${host}${s.path}`
}
window.__serviceUrl = serviceUrl

/**
 * The main (Genesis City) realm: the realm provider's `/main`, composed exactly as the engine's
 * common::structs::default_home_realm. Parcel launches pass it EXPLICITLY so a ?realm override —
 * possibly an invalid world — never leaks into a Places pick (always a Genesis coordinate).
 */
export const DEFAULT_REALM = `${serviceUrl('realmProvider')}/main`
window.__defaultRealm = () => DEFAULT_REALM

// Any other domain arriving by LINK points the whole session's backends — sign-in, content,
// comms — at someone else's servers, so App.tsx stops on it with the UntrustedLaunchGate before
// anything boots (lib/launchGate.ts, which also exempts native: there the shell injects the
// user's own --base-domain flag).
export function isTrustedBaseDomain(domain: string): boolean {
  return TRUSTED_BASE_DOMAINS.includes(domain)
}
