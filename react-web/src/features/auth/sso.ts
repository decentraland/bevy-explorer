// Same-domain single-sign-on, the way `sites` (decentraland.org) does it.
//
// The auth site (decentraland.org/auth) writes the signed AuthIdentity to localStorage
// under `single-sign-on-0x<address>` (via @dcl/single-sign-on-client). Because the HUD is
// served from the SAME ORIGIN as the auth site, that localStorage is shared — so we read
// the identity directly, with no polling and no auth-server request. For a fresh login we
// redirect the browser to `/auth/login?redirectTo=<here>`; the auth site signs in and
// redirects back, by which point the identity is already in localStorage.

import { inShell, postToShell, shellRequest } from '../../lib/shell'
import { GUEST_SESSION_KEY } from './thirdweb'

// The standard Decentraland AuthIdentity, as serialized by @dcl/crypto / @dcl/single-sign-on
// -client. It is the SAME shape no matter how the user signed in (wallet/MetaMask, social,
// OTP, magic) — we just read whatever is stored and forward it; nothing here is method-specific.
// `expiration` is an ISO date string once round-tripped through JSON; authChain[0] is the
// SIGNER (root address), authChain[1] the ECDSA_EPHEMERAL delegate the engine needs.
// `privateKey` is absent from an identity handed out by the web shell, which keeps the key and
// signs for the engine (src/shell/main.ts).
export interface AuthChainLink {
  type: string
  payload: string
  signature: string
}
export interface AuthIdentity {
  ephemeralIdentity: { address: string; publicKey: string; privateKey?: string }
  expiration: string
  authChain: AuthChainLink[]
}

export interface StoredLogin {
  address: string
  identity: AuthIdentity
  /** A guest account the HUD created (features/auth/guest.ts), not a sign-in. */
  guest?: boolean
}

const SSO_PREFIX = 'single-sign-on-'
// The guest account. It is kept apart from the `single-sign-on-*` entries so the other sites on
// this origin don't treat it as a sign-in.
const GUEST_IDENTITY_KEY = 'dcl-guest-identity'

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
  return parseIdentity(localStorage.getItem(SSO_PREFIX + address))
}

function parseIdentity(raw: string | null): AuthIdentity | null {
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

// Scan every `single-sign-on-0x*` entry and the guest, and return the freshest non-expired
// identity (the most recently created session), mirroring sites' getStoredAddress().
export function getStoredLogin(): StoredLogin | null {
  const logins: StoredLogin[] = []
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i)
    if (!key || !key.startsWith(SSO_PREFIX + '0x')) continue
    const address = key.slice(SSO_PREFIX.length)
    const identity = readIdentity(address)
    if (identity) logins.push({ address, identity })
  }
  const guest = getStoredGuestLogin()
  if (guest) logins.push(guest)
  let best: StoredLogin | null = null
  for (const login of logins) {
    if (!best || expirationMs(login.identity) > expirationMs(best.identity)) best = login
  }
  return best
}

export function getStoredGuestLogin(): StoredLogin | null {
  const identity = parseIdentity(localStorage.getItem(GUEST_IDENTITY_KEY))
  return identity ? { address: rootAddress(identity).toLowerCase(), identity, guest: true } : null
}

export function storeGuestLogin(identity: AuthIdentity): void {
  localStorage.setItem(GUEST_IDENTITY_KEY, JSON.stringify(identity))
}

// Forget the guest account. Without its session id the wallet cannot be reached again.
export function clearGuestLogin(): void {
  localStorage.removeItem(GUEST_IDENTITY_KEY)
  localStorage.removeItem(GUEST_SESSION_KEY)
}

export function loginExpired(login: StoredLogin): boolean {
  return expirationMs(login.identity) <= Date.now()
}

// A login without its ephemeral private key.
export function publicLogin(login: StoredLogin | null): StoredLogin | null {
  if (!login) return null
  const { privateKey: _, ...ephemeralIdentity } = login.identity.ephemeralIdentity
  return { ...login, identity: { ...login.identity, ephemeralIdentity } }
}

// Pack an AuthIdentity into a single base64 console-command argument. The engine's
// `/login_identity` command decodes this (root address = authChain[0].payload, ephemeral key or
// address + delegate chain) and finalizes the wallet without any auth-server round-trip.
export function encodeIdentity(identity: AuthIdentity): string {
  return btoa(JSON.stringify(identity))
}

// The signed-in identity, from wherever this page can see it: inside the web shell the shell
// reads it and hands back the public parts (the app never touches the origin's localStorage).
export function getLogin(): Promise<StoredLogin | null> {
  return inShell ? shellRequest<StoredLogin | null>('identity') : Promise.resolve(getStoredLogin())
}

export function clearLogins(): void {
  if (inShell) void shellRequest('logout')
  else clearStoredLogins()
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

// Send the browser to the auth site to sign in (fresh account or switch account). Inside the shell
// the shell navigates, so the auth site gets the top-level page and returns to the shell's URL.
export function redirectToAuth(redirectTo: string = location.href): void {
  if (inShell) postToShell({ type: 'bevy-shell:auth-login' })
  else location.replace(authLoginUrl(redirectTo))
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
