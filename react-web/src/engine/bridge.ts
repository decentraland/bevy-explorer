// Page-side client for the bridge protocol (mock + reference). Transport-agnostic:
// it only touches a BroadcastChannel, so the same client works whether the bridge
// scene runs in this document or a same-origin iframe.
//
// Like dcl-editor's bus, streams/events are delivered through ONE generic
// `on(msg => …)` subscription; only request/response (login) is correlated by id.

import type { AuthIdentity } from '../features/auth/sso'
import {
  BRIDGE_CHANNEL,
  type Envelope,
  type LoginPreviousResult,
  type PageToScene,
  type PreviousLogin,
  type RpcMethod,
  type SceneToPage
} from './protocol'

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void }

export class BridgeClient {
  private readonly ch: BroadcastChannel
  private readonly pending = new Map<string, Pending>()
  private readonly listeners = new Set<(msg: SceneToPage) => void>()

  constructor(channel: string = BRIDGE_CHANNEL) {
    this.ch = new BroadcastChannel(channel)
    this.ch.onmessage = (e: MessageEvent<Envelope>) => {
      const env = e.data
      if (env?.to !== 'page') return // ignore our own / scene-addressed posts
      this.handle(env.msg)
    }
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

  send(msg: PageToScene): void {
    this.ch.postMessage({ to: 'scene', msg } satisfies Envelope)
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
      this.send({ kind: 'rpc:req', id, method })
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
