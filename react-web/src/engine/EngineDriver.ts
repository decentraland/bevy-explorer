// Real engine session driver.
//   - Login ACTIONS go over engine-native console commands (no scene needed):
//     /login_guest, /login_previous, /logout  (agent_commands.rs)
//   - STREAMS/events (scene loading, player-ready, chat, chat visibility) arrive
//     from the super-user bridge scene over the `bevy-ui-bridge` BroadcastChannel
//     and are delivered through ONE generic `on(msg => …)` subscription.

import { getStoredLogin, rootAddress, type AuthIdentity } from '../features/auth/sso'
import type { LoginDriver } from './driver'
import type { EngineRpc } from './engineRpc'
import {
  BRIDGE_CHANNEL,
  type Envelope,
  type PageToScene,
  type SceneToPage
} from './protocol'

// Pack a same-domain SSO AuthIdentity into a single base64 console-command argument. The
// engine's `/login_identity` command decodes this (root address = authChain[0].payload,
// ephemeral key + delegate chain) and finalizes the wallet without any auth-server round-trip.
function encodeIdentity(identity: AuthIdentity): string {
  return btoa(JSON.stringify(identity))
}

export class EngineDriver implements LoginDriver {
  private readonly ch: BroadcastChannel
  private readonly listeners = new Set<(msg: SceneToPage) => void>()
  private playerReadyFired = false

  constructor(private readonly rpc: EngineRpc) {
    this.ch = new BroadcastChannel(BRIDGE_CHANNEL)
    this.ch.onmessage = (e: MessageEvent<Envelope>) => {
      const env = e.data
      if (env?.to !== 'page') return
      if (env.msg.kind === 'event' && env.msg.name === 'playerReady') {
        this.playerReadyFired = true
      }
      this.emit(env.msg)
    }
  }

  async getPreviousLogin(): Promise<{ userId: string | null }> {
    // The engine has no console command to query a saved login, but a same-domain SSO identity
    // in localStorage is exactly that — a previous login we can hand back via `/login_identity`.
    const login = getStoredLogin()
    return { userId: login ? rootAddress(login.identity) : null }
  }

  async loginPrevious(): Promise<unknown> {
    const r = await this.rpc.command('/login_previous')
    this.scheduleReadyFallback()
    return r
  }

  async loginGuest(): Promise<void> {
    await this.rpc.command('/login_guest')
    this.scheduleReadyFallback()
  }

  async loginCancel(): Promise<void> {
    // SSO login is a redirect/console-command; there is no in-engine flow to cancel.
  }

  async logout(): Promise<void> {
    await this.rpc.command('/logout')
  }

  async loginWithIdentity(identity: AuthIdentity): Promise<void> {
    await this.rpc.command(`/login_identity ${encodeIdentity(identity)}`)
    this.scheduleReadyFallback()
  }

  // "Jump in": reuse the SSO identity via `/login_identity`; if none is stored, fall back to
  // the engine's own saved login.
  async jumpIn(): Promise<void> {
    const login = getStoredLogin()
    if (login) await this.loginWithIdentity(login.identity)
    else await this.loginPrevious()
  }

  send(msg: PageToScene): void {
    this.ch.postMessage({ to: 'scene', msg } satisfies Envelope)
  }

  on(fn: (msg: SceneToPage) => void): () => void {
    this.listeners.add(fn)
    return () => this.listeners.delete(fn)
  }

  dispose(): void {
    this.ch.close()
    this.listeners.clear()
  }

  renderBusy(): boolean {
    return this.rpc.renderBusy()
  }

  private emit(msg: SceneToPage): void {
    this.listeners.forEach((fn) => fn(msg))
  }

  // The bridge scene emits a precise playerReady; until it does, hand off to the
  // engine a few seconds after login so the world isn't hidden forever.
  private scheduleReadyFallback(): void {
    setTimeout(() => {
      if (!this.playerReadyFired) {
        this.playerReadyFired = true
        this.emit({ kind: 'event', name: 'playerReady' })
      }
    }, 6000)
  }
}
