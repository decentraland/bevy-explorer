// Shared test harness for the deterministic domain suite (tier 1).
//
// `FakeDriver` implements the real LoginDriver interface but talks to nothing — it
// records every page→scene API call (`sent`) and lets a test inject scene→page
// messages (`emit`). Driving the real `useEngineSession` hook through it lets each
// domain test assert: (a) every action posts the exact wire message, and (b) every
// inbound message kind updates the session state. No engine, no BroadcastChannel.

import { renderHook, act, waitFor, type RenderHookResult } from '@testing-library/react'
import { expect } from 'vitest'
import type { LoginDriver } from '../engine/driver'
import type { PageToScene, SceneToPage } from '../engine/protocol'
import { useEngineSession, type EngineSession } from '../features/session/useEngineSession'

export class FakeDriver implements LoginDriver {
  /** Every page→scene message posted via `send` (the API calls under test). */
  readonly sent: PageToScene[] = []
  /** Login-method invocations, in order (getPreviousLogin/loginGuest/jumpIn/logout/…). */
  readonly calls: string[] = []
  /** What getPreviousLogin resolves to (set before render to drive the login branch). */
  previousLogin: { userId: string | null } = { userId: null }

  private readonly listeners = new Set<(msg: SceneToPage) => void>()

  async getPreviousLogin(): Promise<{ userId: string | null }> {
    this.calls.push('getPreviousLogin')
    return this.previousLogin
  }
  async loginPrevious(): Promise<unknown> {
    this.calls.push('loginPrevious')
    return undefined
  }
  async loginGuest(): Promise<void> {
    this.calls.push('loginGuest')
  }
  async loginCancel(): Promise<void> {
    this.calls.push('loginCancel')
  }
  async logout(): Promise<void> {
    this.calls.push('logout')
  }
  async loginWithIdentity(): Promise<void> {
    this.calls.push('loginWithIdentity')
  }
  async jumpIn(): Promise<void> {
    this.calls.push('jumpIn')
  }
  send(msg: PageToScene): void {
    this.sent.push(msg)
  }
  on(fn: (msg: SceneToPage) => void): () => void {
    this.listeners.add(fn)
    return () => this.listeners.delete(fn)
  }
  dispose(): void {
    this.listeners.clear()
  }

  // ---- test helpers --------------------------------------------------------

  /** Push a scene→page message to the hook (wrapped in act so state settles). */
  emit(msg: SceneToPage): void {
    act(() => {
      this.listeners.forEach((fn) => fn(msg))
    })
  }

  /** All sent messages of a given kind. */
  sentOf<K extends PageToScene['kind']>(kind: K): Extract<PageToScene, { kind: K }>[] {
    return this.sent.filter((m) => m.kind === kind) as Extract<PageToScene, { kind: K }>[]
  }

  /** The most recent sent message of a given kind (or undefined). */
  last<K extends PageToScene['kind']>(kind: K): Extract<PageToScene, { kind: K }> | undefined {
    for (let i = this.sent.length - 1; i >= 0; i--) {
      if (this.sent[i].kind === kind) return this.sent[i] as Extract<PageToScene, { kind: K }>
    }
    return undefined
  }

  /** Forget everything sent so far (e.g. after world-entry's auto getProfile/getNotifications). */
  clearSent(): void {
    this.sent.length = 0
  }
}

export interface Harness {
  driver: FakeDriver
  result: RenderHookResult<EngineSession, unknown>['result']
  /** The current session value (re-read after acts). */
  session: () => EngineSession
}

/** Render the real session hook wired to a FakeDriver. */
export function renderSession(previousLogin?: { userId: string | null }): Harness {
  const driver = new FakeDriver()
  if (previousLogin) driver.previousLogin = previousLogin
  const { result } = renderHook(() => useEngineSession(() => driver))
  return { driver, result, session: () => result.current }
}

/**
 * Drive the session from login → world (the e2e "enter as guest" step):
 * waits for the login screen, clicks guest, then emits playerReady. Leaves the
 * driver's `sent` cleared of the world-entry auto-fetches unless `keepSent`.
 */
export async function enterAsGuest(h: Harness, opts: { keepSent?: boolean } = {}): Promise<void> {
  await waitFor(() => expect(h.session().login.status).not.toBe('loading'))
  act(() => h.session().login.exploreAsGuest())
  await waitFor(() => expect(h.session().phase).toBe('entering'))
  h.driver.emit({ kind: 'event', name: 'playerReady' })
  await waitFor(() => expect(h.session().phase).toBe('world'))
  if (!opts.keepSent) h.driver.clearSent()
}
