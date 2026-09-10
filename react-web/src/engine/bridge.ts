// Page-side client for the bridge protocol (mock + reference). Transport-agnostic:
// it only touches a BroadcastChannel, so the same client works wherever the bridge
// scene runs (its JS isolate, same-origin).
//
// Like dcl-editor's bus, streams/events are delivered through ONE generic
// `on(msg => …)` subscription; only request/response (login) is correlated by id.

import type { AuthIdentity } from '../features/auth/sso'
import { BridgeChannel } from './bridgeChannel'
import {
  bridgeChannelName,
  type LoginPreviousResult,
  type PageToScene,
  type PreviousLogin,
  type RpcMethod,
  type SceneToPage
} from './protocol'

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void }

export class BridgeClient {
  private readonly ch: BridgeChannel
  private readonly pending = new Map<string, Pending>()
  private readonly listeners = new Set<(msg: SceneToPage) => void>()

  constructor(channel: string = bridgeChannelName()) {
    this.ch = new BridgeChannel(
      channel,
      (msg) => this.handle(msg),
      () => this.handle({ kind: 'bridgeUnavailable' })
    )
  }

  getPreviousLogin(): Promise<PreviousLogin> {
    return this.rpc<PreviousLogin>('getPreviousLogin')
  }

  loginPrevious(): Promise<LoginPreviousResult> {
    return this.rpc<LoginPreviousResult>('loginPrevious')
  }

  loginGuest(): Promise<void> {
    return this.rpc<void>('loginGuest')
  }

  loginCancel(): Promise<void> {
    return this.rpc<void>('loginCancel')
  }

  logout(): Promise<void> {
    return this.rpc<void>('logout')
  }

  // Hand the same-domain SSO identity to the engine. The mock just acknowledges and spawns
  // the player; the real path is EngineDriver's `/login_identity` console command.
  loginWithIdentity(_identity: AuthIdentity): Promise<void> {
    return this.rpc<void>('loginIdentity')
  }

  // "Jump in": reuse the existing login. The engine (BevyApi) has no log-in-with-raw-identity
  // surface — only `loginPrevious` — so jump-in maps to that. (`loginIdentity` would hit the
  // bridge scene's default case and throw "unsupported method".)
  async jumpIn(): Promise<void> {
    const r = await this.rpc<LoginPreviousResult>('loginPrevious')
    if (r && r.success === false) throw new Error(r.error || 'Could not reuse your login')
  }

  // Fresh sign-in via the engine's remote-wallet flow: the verification code arrives
  // mid-flight as a 'loginCode' message (generic `on` subscription); this resolves once the
  // user approves in the external browser the engine opened.
  async loginNew(): Promise<void> {
    const r = await this.rpc<LoginPreviousResult>('loginNew')
    if (r && r.success === false) throw new Error(r.error || 'Sign-in failed')
  }

  send(msg: PageToScene): void {
    this.ch.send(msg)
  }

  expectBridge(): void {
    this.ch.expectReady()
  }

  on(fn: (msg: SceneToPage) => void): () => void {
    this.listeners.add(fn)
    return () => this.listeners.delete(fn)
  }

  dispose(): void {
    this.ch.close()
    this.pending.clear()
    this.listeners.clear()
  }

  private rpc<T>(method: RpcMethod): Promise<T> {
    const id = crypto.randomUUID()
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject })
      // The engine's boot shim answers these before any bridge scene exists, so they skip the
      // handshake queue rather than gating the login screen on a scene sign-in does not involve.
      this.ch.sendNow({ kind: 'rpc:req', id, method })
    })
  }

  // rpc replies are correlated here; everything else fans out to `on` listeners.
  private handle(msg: SceneToPage): void {
    if (msg.kind === 'rpc:res') {
      const p = this.pending.get(msg.id)
      if (!p) return
      this.pending.delete(msg.id)
      if (msg.ok) p.resolve(msg.value)
      else p.reject(new Error(msg.error ?? 'bridge rpc failed'))
      return
    }
    this.listeners.forEach((fn) => fn(msg))
  }
}
