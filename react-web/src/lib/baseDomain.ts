// The base domain every HUD backend host is composed from:
//   1. the ?baseDomain= entry param (set by hand on web, injected by the native shell from
//      --base-domain; non-decentraland values are stopped by the UntrustedLaunchGate)
//   2. else derived from the hosting origin — the staging deployment at decentraland.zone/bevy-web
//      keys to zone backends, prod to org
//   3. else decentraland.org (localhost dev, the native CEF page, everything else)
// Captured once from the entry URL; the engine's URL syncs preserve unknown params so the
// param form survives realm/position rewrites.
// MIRROR: deploy/web/engine/boot.js derives the same value the same way for the engine side,
// and crates/common/src/base_domain.rs is the engine-internal equivalent.

/** The apex deployment domain a hosting origin implies, or null for unrecognised hosts. */
export function hostBaseDomain(hostname: string): string | null {
  for (const apex of ['decentraland.org', 'decentraland.zone']) {
    if (hostname === apex || hostname.endsWith(`.${apex}`)) return apex
  }
  return null
}

export const BASE_DOMAIN =
  new URLSearchParams(window.location.search).get('baseDomain') ??
  hostBaseDomain(window.location.hostname) ??
  'decentraland.org'

/** https origin for a service subdomain, e.g. serviceUrl('places') → "https://places.decentraland.org". */
export function serviceUrl(sub: string): string {
  return `https://${sub}.${BASE_DOMAIN}`
}

// Decentraland's own deployments. Any other domain arriving by LINK points the whole session's
// backends — sign-in, content, comms — at someone else's servers, so App.tsx stops on it with the
// UntrustedLaunchGate before anything boots. The native shell injects the param from the user's
// own --base-domain flag (App runs in native mode there), which needs no gate.
export function isTrustedBaseDomain(domain: string): boolean {
  return domain === 'decentraland.org' || domain === 'decentraland.zone'
}
