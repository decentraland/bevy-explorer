// The session's screen events that need memory across renders: the loading episode (mobile's
// loading_screen.gd: LOADING_START / LOADING_END / LOADING_TIMEOUT) and the login type a deferred
// login is reported with.

import { screenViewed } from '../../lib/analytics'
import type { StoredLogin } from '../auth/sso'
import type { Destination } from './useEngineSession'

export type LoginType = 'guest' | 'fully_registered'

/** A reused login's kind: the shell's stored login when there is one (the driver signs in with
 *  it first), else the engine's saved login. */
export function storedLoginType(stored: StoredLogin | null, engineGuest: boolean): LoginType {
  const guest = stored != null ? stored.guest === true : engineGuest
  return guest ? 'guest' : 'fully_registered'
}

/** The realm the page is in, as the engine keeps the url (`?realm=`) in step with it. */
export function currentRealm(defaultRealm: string): string {
  return new URLSearchParams(location.search).get('realm') ?? defaultRealm
}

// Mobile gives up on a load after this long and reports it.
export const LOADING_TIMEOUT_MS = 90_000

export interface LoadingTarget {
  realm: string
  position: string
}

/** Where a picked destination leads, as the loading events name it. */
export function loadingTargetOf(dest: Destination, defaultRealm: string): LoadingTarget {
  if (dest == null) return { realm: defaultRealm, position: 'home' }
  if (dest.kind === 'parcel') return { realm: defaultRealm, position: `${dest.x},${dest.y}` }
  return { realm: dest.realm, position: dest.position ?? 'unknown' }
}

export interface LoadingEpisode {
  /** A load the user asked for began (null: the engine streaming on its own, not reported); a
   *  second start while one is open is the same episode. */
  start(target: LoadingTarget | null): void
  /** The load ended, by reaching the world or by failing; nothing when none is open. */
  end(status: 'Success' | 'Failed', position?: string): void
  /** The load is waiting on the user (the welcome page): the episode stays open, the clock stops. */
  pause(): void
  /** The next start is the first load of a new login. */
  resetLogin(): void
  dispose(): void
}

export function createLoadingEpisode(): LoadingEpisode {
  let open = false
  let firstOfLogin = true
  let position = 'unknown'
  let timer: ReturnType<typeof setTimeout> | null = null

  const clear = (): void => {
    if (timer != null) clearTimeout(timer)
    timer = null
  }
  const arm = (): void => {
    timer = setTimeout(() => {
      timer = null
      screenViewed('LOADING_TIMEOUT', { total_elapsed_seconds: LOADING_TIMEOUT_MS / 1000, timeout_reason: 'max_time' })
    }, LOADING_TIMEOUT_MS)
  }

  return {
    start(target) {
      if (open) {
        if (timer == null) arm()
        return
      }
      if (target == null) return
      open = true
      position = target.position
      screenViewed('LOADING_START', {
        position,
        realm: target.realm,
        when: firstOfLogin ? 'on_explorer_ready' : 'on_teleport'
      })
      firstOfLogin = false
      arm()
    },
    end(status, at) {
      if (!open) return
      open = false
      clear()
      screenViewed('LOADING_END', { position: at ?? position, status })
    },
    pause: clear,
    resetLogin() {
      firstOfLogin = true
    },
    dispose: clear
  }
}
