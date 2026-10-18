// The login type a deferred login is reported with.

import type { StoredLogin } from '../auth/sso'

export type LoginType = 'guest' | 'fully_registered'

export function storedLoginType(stored: StoredLogin | null): LoginType {
  return stored?.guest === true ? 'guest' : 'fully_registered'
}
