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
