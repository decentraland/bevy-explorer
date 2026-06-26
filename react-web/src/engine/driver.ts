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
  loginPrevious(): Promise<unknown>
  loginGuest(): Promise<void>
  loginCancel(): Promise<void>
  /** Sign out of the current account → back to the login screen. */
  logout(): Promise<void>
  /** Hand a same-domain SSO identity to the engine (replaces the auth-server poll). */
  loginWithIdentity(identity: AuthIdentity): Promise<void>
  /** Reuse the existing login (the "Jump in" button). Each backend logs in the way it
   *  supports: console `/login_identity` for the engine, `loginPrevious` over the bridge. */
  jumpIn(): Promise<void>
  /** Post a message to the scene (e.g. sendChat). */
  send(msg: PageToScene): void
  /** Subscribe to every scene→page message. Returns an unsubscribe. */
  on(fn: (msg: SceneToPage) => void): () => void
  dispose(): void
}
