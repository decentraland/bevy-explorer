// Page-side client for the bridge protocol (mock + reference). Transport-agnostic:
// it only touches a BroadcastChannel, so the same client works wherever the bridge
// scene runs (its JS isolate, same-origin).
//
// Like dcl-editor's bus, streams/events are delivered through ONE generic
// `on(msg => …)` subscription; only request/response (login) is correlated by id.

import { createGuestIdentity } from '../features/auth/guest'
import { encodeIdentity, type AuthIdentity } from '../features/auth/sso'
import type { GuestWallet } from '../features/auth/thirdweb'
import { BridgeChannel } from './bridgeChannel'
import type { LoginDriver } from './driver'
import {
  bridgeChannelName,
  type LoginPreviousResult,
  type PageToScene,
  type PreviousLogin,
  type RpcMethod,
  type RpcRequest,
  type SceneToPage
} from './protocol'

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void; timer?: ReturnType<typeof setTimeout> }

// Local reads/actions only: sign-ins wait on the network or the user's external browser.
const QUICK_RPC_TIMEOUT_MS: Partial<Record<RpcMethod, number>> = {
  getPreviousLogin: 10_000,
  loginCancel: 10_000,
  logout: 10_000,
  guestDiscard: 10_000
}

// Answered by the bridge scene, not the engine's boot shim.
const SCENE_RPCS: ReadonlySet<RpcMethod> = new Set(['guestLogin', 'guestSign', 'guestDiscard'])

export class BridgeClient implements LoginDriver {
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

  // Native: the bridge scene makes the thirdweb calls and keeps the session id (the page's
  // cef:// origin is not one thirdweb accepts); the identity then goes in through the engine
  // console. The mock has no engine console, so it stays a plain guest.
  async loginPersistentGuest(): Promise<void> {
    if (!('engine_console_command' in window)) return this.loginGuest()
    const identity = await createGuestIdentity({
      login: (sessionId) => this.rpc<GuestWallet>('guestLogin', { sessionId }),
      sign: (token, message) => this.rpc<string>('guestSign', { token, message })
    })
    await this.command(`/login_identity ${encodeIdentity(identity)} --guest`)
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
  async jumpIn(defaultOnError = false): Promise<void> {
    // The scene's loginPrevious can't carry the flag; the engine console can.
    if (defaultOnError) {
      await this.command('/login_previous --default-on-error')
      return
    }
    const r = await this.rpc<LoginPreviousResult>('loginPrevious')
    if (r && r.success === false) throw new Error(r.error || 'Could not reuse your login')
  }

  // Fresh sign-in via the engine's remote-wallet flow: the verification code arrives
  // mid-flight as a 'loginCode' message (generic `on` subscription); this resolves once the
  // user approves in the external browser the engine opened.
  async loginNew(): Promise<void> {
    const r = await this.rpc<LoginPreviousResult>('loginNew')
    if (r && r.success === false) throw new Error(r.error || 'Sign-in failed')
    // Signing in with an account replaces the guest.
    this.rpc<void>('guestDiscard').catch(() => {})
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
    for (const p of this.pending.values()) {
      clearTimeout(p.timer)
      p.reject(new Error('bridge closed'))
    }
    this.pending.clear()
    this.listeners.clear()
  }

  // Native: the CEF shim installs window.engine_console_command (see lib/cefNativeBridge). The
  // mock has no engine console.
  command(line: string): Promise<string> {
    const run = (window as Window & { engine_console_command?: (line: string) => Promise<string> }).engine_console_command
    return run ? run(line) : Promise.reject(new Error('engine console not available'))
  }

  private rpc<T>(method: RpcMethod, params?: Record<string, string>): Promise<T> {
    const id = crypto.randomUUID()
    return new Promise<T>((resolve, reject) => {
      const limit = QUICK_RPC_TIMEOUT_MS[method]
      const timer = limit == null ? undefined : setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`bridge rpc ${method} timed out`))
      }, limit)
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject, timer })
      // The engine's boot shim answers the login rpcs before any bridge scene exists, so they skip
      // the handshake queue rather than gating the login screen on a scene sign-in does not
      // involve. The guest rpcs are the scene's, so they wait for it.
      const msg: RpcRequest = { kind: 'rpc:req', id, method, params }
      if (SCENE_RPCS.has(method)) this.ch.send(msg)
      else this.ch.sendNow(msg)
    })
  }

  // rpc replies are correlated here; everything else fans out to `on` listeners.
  private handle(msg: SceneToPage): void {
    if (msg.kind === 'rpc:res') {
      const p = this.pending.get(msg.id)
      if (!p) return
      this.pending.delete(msg.id)
      clearTimeout(p.timer)
      if (msg.ok) p.resolve(msg.value)
      else p.reject(new Error(msg.error ?? 'bridge rpc failed'))
      return
    }
    this.listeners.forEach((fn) => fn(msg))
  }
}
