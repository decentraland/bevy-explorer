import type { AuthIdentity } from '../features/auth/sso'
import type { PageToScene, SceneToPage } from './protocol'

// The session surface the UI depends on. Two implementations:
//   - BridgeClient   (mock, over BroadcastChannel)
//   - EngineDriver   (console commands for login actions + bridge scene for streams)
// The UI is written against this interface so it doesn't care which.
//
// Streams/events use ONE generic subscription (like dcl-editor's onSceneMessage):
// the consumer subscribes once and switches on `msg.kind`. Login stays typed
// because the two implementations drive it differently (console vs rpc).

export interface LoginDriver {
  getPreviousLogin(): Promise<{ userId: string | null }>
  loginPrevious(defaultOnError?: boolean): Promise<unknown>
  loginGuest(): Promise<void>
  loginCancel(): Promise<void>
  /** Sign out of the current account → back to the login screen. */
  logout(): Promise<void>
  /** Hand a same-domain SSO identity to the engine (replaces the auth-server poll).
   *  `defaultOnError` continues with (and deploys) a default profile if the user's current
   *  profile can't be fetched — it OVERWRITES the server-side profile, so only pass it after
   *  a failed attempt, with explicit user consent. */
  loginWithIdentity(identity: AuthIdentity, defaultOnError?: boolean): Promise<void>
  /** Reuse the existing login (the "Jump in" button). Each backend logs in the way it
   *  supports: console `/login_identity` for the engine, `loginPrevious` over the bridge.
   *  `defaultOnError` as for loginWithIdentity. */
  jumpIn(defaultOnError?: boolean): Promise<void>
  /** Post a message to the scene (e.g. sendChat). */
  send(msg: PageToScene): void
  /** Subscribe to every scene→page message. Returns an unsubscribe. */
  on(fn: (msg: SceneToPage) => void): () => void
  dispose(): void
  /** True while the engine is still rendering the just-loaded scene (shaders compiling). The session
   *  holds the loading screen until this clears so the world isn't revealed as black models.
   *  Optional — the mock has no engine to wait for. */
  renderBusy?(): boolean
  /** True once the engine is warm enough to launch (WASM compiled + GPU cache ready). The login
   *  screen keeps its CTAs in a "Starting…" state until this is true. Optional — the mock is always
   *  ready. */
  engineReady?(): boolean
  /** Real weighted boot progress (0–100) + active step id, surfaced from the engine iframe loader for
   *  the login footer bar. Optional — the mock has no engine to download. */
  loadProgress?(): number
  loadStep?(): string | null
  /** Last Rust panic text captured from the engine, or null. Optional — the mock has no engine. */
  enginePanic?(): { message: string } | null
  /** Clear the stashed panic once consumed, so a later read can't surface a stale one. Optional — mock. */
  clearEnginePanic?(): void
  /** Re-arm the iframe crash watchdog after the host dismisses a runtime crash (resets its `shown`
   *  flag so a second genuine crash still shows). Optional — the mock has no engine. */
  rearmCrashWatchdog?(): void
  /** Boot the engine at a chosen realm/position (deferred-start: nothing loads until the user picks
   *  a destination). A parcel passes `position` "x,y"; a world passes `realm`; skip passes "0,0".
   *  Optional — the mock has no engine to launch. */
  launch?(realm?: string, position?: string): void
}
