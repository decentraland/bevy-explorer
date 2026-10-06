// The signed-in identity, kept by the shell for the app (src/lib/shell.ts shellRequest): the app
// gets the public identity and asks for signatures, and never sees the key.
import { createGuestIdentity, webGuestBackend } from '../features/auth/guest'
import {
  clearGuestLogin,
  clearStoredLogins,
  getStoredGuestLogin,
  getStoredLogin,
  loginExpired,
  publicLogin,
  rootAddress,
  storeGuestLogin,
  type StoredLogin
} from '../features/auth/sso'
import { personalSign } from './sign'

// The login handed to the app, key included. It is the only one the shell signs with, and once it
// has signed it is kept for the session, so signing out or in again in another tab doesn't pull it
// from under the running engine. Until then (the login screen), or once it expires, storage is
// read again.
let held: StoredLogin | null = null
let pinned = false
function currentLogin(): StoredLogin | null {
  if (!pinned || !held || loginExpired(held)) {
    held = getStoredLogin()
    pinned = false
  }
  return held
}

export async function request(method: unknown, params: unknown): Promise<unknown> {
  switch (method) {
    case 'identity':
      return publicLogin(currentLogin())
    case 'sign': {
      const { signer, message } = (params ?? {}) as { signer?: unknown; message?: unknown }
      if (typeof signer !== 'string' || typeof message !== 'string') throw new Error('bad sign request')
      const login = currentLogin()
      const ephemeral = login?.identity.ephemeralIdentity
      if (!login || !ephemeral?.privateKey || ephemeral.address.toLowerCase() !== signer.toLowerCase()) {
        throw new Error(`not signed in as ${signer}`)
      }
      pinned = true
      // Signing in with an account replaces the guest.
      if (!login.guest && getStoredGuestLogin()) clearGuestLogin()
      return personalSign(ephemeral.privateKey, message)
    }
    // "Explore as guest": a guest account that persists across visits (features/auth/guest.ts),
    // created and kept here like a sign-in. Any earlier guest is replaced.
    case 'createGuest': {
      const identity = await createGuestIdentity(webGuestBackend)
      storeGuestLogin(identity)
      held = { address: rootAddress(identity).toLowerCase(), identity, guest: true }
      pinned = true
      return publicLogin(held)
    }
    // Sign out. The guest is kept: it is only reachable through its stored session id.
    case 'logout':
      clearStoredLogins()
      held = null
      pinned = false
      return null
    default:
      throw new Error(`unknown request ${String(method)}`)
  }
}
