// The base domain every HUD backend host is composed from — the ?baseDomain= entry param
// (set by hand on web, injected by the native shell from --base-domain), defaulting to
// decentraland.org. Captured once from the entry URL; the engine's URL syncs preserve unknown
// params so it survives realm/position rewrites. Mirrors the engine's common::base_domain
// (crates/common/src/base_domain.rs).
export const BASE_DOMAIN = new URLSearchParams(window.location.search).get('baseDomain') ?? 'decentraland.org'

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
