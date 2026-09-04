// The base domain every HUD backend host is composed from:
//   1. the ?baseDomain= entry param (set by hand on web, injected by the native shell from
//      --base-domain; non-decentraland values are stopped by the UntrustedLaunchGate)
//   2. else derived from the hosting origin — the staging deployment at decentraland.zone/bevy-web
//      keys to zone backends, prod to org
//   3. else decentraland.org (localhost dev, the native CEF page, everything else)
// Captured once from the entry URL; the engine's URL syncs preserve unknown params so the
// param form survives realm/position rewrites. This module is the single source: it publishes
// the value as window.__baseDomain() for the engine loader (deploy/web/engine/boot.js) and the
// wasm (src/web.rs apply_base_domain → crates/common/src/base_domain.rs), both of which run
// only after the HUD has mounted EngineHost.

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

export const BASE_DOMAIN =
  normaliseBaseDomain(new URLSearchParams(window.location.search).get('baseDomain')) ??
  hostBaseDomain(window.location.hostname) ??
  'decentraland.org'

declare global {
  interface Window {
    __baseDomain?: () => string
  }
}
window.__baseDomain = () => BASE_DOMAIN

/** https origin for a service subdomain, e.g. serviceUrl('places') → "https://places.decentraland.org". */
export function serviceUrl(sub: string): string {
  return `https://${sub}.${BASE_DOMAIN}`
}

// Any other domain arriving by LINK points the whole session's backends — sign-in, content,
// comms — at someone else's servers, so App.tsx stops on it with the UntrustedLaunchGate before
// anything boots (lib/launchGate.ts, which also exempts native: there the shell injects the
// user's own --base-domain flag).
export function isTrustedBaseDomain(domain: string): boolean {
  return TRUSTED_BASE_DOMAINS.includes(domain)
}
