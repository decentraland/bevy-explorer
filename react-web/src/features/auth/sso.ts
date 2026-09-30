// Same-domain single-sign-on, the way `sites` (decentraland.org) does it.
//
// The auth site (decentraland.org/auth) writes the signed AuthIdentity to localStorage
// under `single-sign-on-0x<address>` (via @dcl/single-sign-on-client). Because the HUD is
// served from the SAME ORIGIN as the auth site, that localStorage is shared — so we read
// the identity directly, with no polling and no auth-server request. For a fresh login we
// redirect the browser to `/auth/login?redirectTo=<here>`; the auth site signs in and
// redirects back, by which point the identity is already in localStorage.

// The standard Decentraland AuthIdentity, as serialized by @dcl/crypto / @dcl/single-sign-on
// -client. It is the SAME shape no matter how the user signed in (wallet/MetaMask, social,
// OTP, magic) — we just read whatever is stored and forward it; nothing here is method-specific.
// `expiration` is an ISO date string once round-tripped through JSON; authChain[0] is the
// SIGNER (root address), authChain[1] the ECDSA_EPHEMERAL delegate the engine needs.
export interface AuthChainLink {
  type: string
  payload: string
  signature: string
}
export interface AuthIdentity {
  ephemeralIdentity: { address: string; publicKey: string; privateKey: string }
  expiration: string
  authChain: AuthChainLink[]
}

export interface StoredLogin {
  address: string
  identity: AuthIdentity
}

const SSO_PREFIX = 'single-sign-on-'

// Expiration: prefer the top-level field; fall back to the `Expiration: <date>` line in the
// ECDSA_EPHEMERAL payload (what `sites` parses), for identities stored without the top field.
function expirationMs(identity: AuthIdentity): number {
  const top = identity.expiration ? new Date(identity.expiration).getTime() : NaN
  if (!Number.isNaN(top)) return top
  const payload = identity.authChain?.[1]?.payload
  const m = payload ? /Expiration:\s*([^\n]+)/.exec(payload) : null
  return m ? new Date(m[1]).getTime() : 0
}

function isHexAddress(address: string): boolean {
  return /^0x[0-9a-fA-F]{40}$/.test(address)
}

function readIdentity(address: string): AuthIdentity | null {
  if (!isHexAddress(address)) return null
  const raw = localStorage.getItem(SSO_PREFIX + address)
  if (!raw) return null
  try {
    const identity = JSON.parse(raw) as AuthIdentity
    if (!identity?.ephemeralIdentity?.privateKey || !Array.isArray(identity.authChain)) return null
    if (expirationMs(identity) <= Date.now()) return null
    // Non-hex addresses (e.g. the mock bridge's 0xmock… seed) would fail the engine's login parse.
    const signer = identity.authChain.find((l) => l.type === 'SIGNER')
    if (!signer || !isHexAddress(signer.payload?.trim() ?? '')) return null
    return identity
  } catch {
    return null
  }
}

// Scan every `single-sign-on-0x*` entry and return the freshest non-expired identity (the
// most recently created session), mirroring sites' getStoredAddress().
export function getStoredLogin(): StoredLogin | null {
  let best: { address: string; identity: AuthIdentity; exp: number } | null = null
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i)
    if (!key || !key.startsWith(SSO_PREFIX + '0x')) continue
    const address = key.slice(SSO_PREFIX.length)
    const identity = readIdentity(address)
    if (!identity) continue
    const exp = expirationMs(identity)
    if (!best || exp > best.exp) best = { address, identity, exp }
  }
  return best ? { address: best.address, identity: best.identity } : null
}

// The user's root (wallet) address for a stored identity = the SIGNER link payload.
export function rootAddress(identity: AuthIdentity): string {
  return identity.authChain?.[0]?.payload ?? ''
}

// Same-origin auth site. Redirecting here keeps us on decentraland.org/.zone/.today so the
// identity it writes lands in this origin's localStorage.
export function authLoginUrl(redirectTo: string = location.href): string {
  return `/auth/login?redirectTo=${encodeURIComponent(redirectTo)}`
}

// Send the browser to the auth site to sign in (fresh account or switch account).
export function redirectToAuth(redirectTo: string = location.href): void {
  location.replace(authLoginUrl(redirectTo))
}

// Sign out: drop every SSO identity for this origin (matches sites' disconnect for the
// single-sign-on-* keys). The engine logout is handled separately by the driver.
export function clearStoredLogins(): void {
  const keys: string[] = []
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i)
    if (key && key.startsWith(SSO_PREFIX)) keys.push(key)
  }
  keys.forEach((k) => localStorage.removeItem(k))
}
