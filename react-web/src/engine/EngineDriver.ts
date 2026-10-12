// Real engine session driver.
//   - Login ACTIONS go over engine-native console commands (no scene needed):
//     /login_guest, /login_previous, /logout  (agent_commands.rs)
//   - STREAMS/events (scene loading, player-ready, chat, chat visibility) arrive
//     from the super-user bridge scene over the `bevy-ui-bridge` BroadcastChannel
//     and are delivered through ONE generic `on(msg => …)` subscription.

import { encodeIdentity, getLogin, rootAddress, type AuthIdentity, type StoredLogin } from '../features/auth/sso'
import { inShell, shellRequest } from '../lib/shell'
import type { LoginDriver } from './driver'
import type { EngineRpc, LaunchHostOptions } from './engineRpc'
import { BridgeChannel } from './bridgeChannel'
import { bridgeChannelName, type PageToScene, type SceneToPage } from './protocol'

// Two thirdweb round-trips.
const CREATE_GUEST_TIMEOUT_MS = 30_000

export class EngineDriver implements LoginDriver {
  private readonly ch: BridgeChannel
  private readonly listeners = new Set<(msg: SceneToPage) => void>()
  private playerReadyFired = false
  private readyFallbackTimer: ReturnType<typeof setTimeout> | null = null

  constructor(private readonly rpc: EngineRpc) {
    this.ch = new BridgeChannel(
      bridgeChannelName(),
      (msg) => {
        if (msg.kind === 'event' && msg.name === 'playerReady') this.playerReadyFired = true
        this.emit(msg)
      },
      () => this.emit({ kind: 'bridgeUnavailable' })
    )
  }

  async getPreviousLogin(): Promise<{ userId: string | null }> {
    // The engine has no console command to query a saved login, but a same-domain SSO identity
    // in localStorage is exactly that — a previous login we can hand back via `/login_identity`.
    const login = await getLogin()
    return { userId: login ? rootAddress(login.identity) : null }
  }

  async loginPrevious(defaultOnError?: boolean): Promise<unknown> {
    const flag = defaultOnError === true ? ' --default-on-error' : ''
    const r = await this.rpc.command(`/login_previous${flag}`)
    this.scheduleReadyFallback()
    return r
  }

  async loginGuest(): Promise<void> {
    await this.rpc.command('/login_guest')
    this.scheduleReadyFallback()
  }

  // The shell creates the guest account and keeps it, key included, like a sign-in; the engine
  // gets the public identity and signs through the shell. Outside the shell (a credentialless
  // embed) there is nowhere to keep it, so the guest is a throwaway one.
  async loginPersistentGuest(): Promise<void> {
    if (!inShell) return this.loginGuest()
    const login = await shellRequest<StoredLogin>('createGuest', undefined, CREATE_GUEST_TIMEOUT_MS)
    await this.loginWithIdentity(login.identity, false, true)
  }

  async loginCancel(): Promise<void> {
    // SSO login is a redirect/console-command; there is no in-engine flow to cancel.
  }

  async logout(): Promise<void> {
    // The next account spawns anew, so its world handoff must fire again.
    this.playerReadyFired = false
    if (this.readyFallbackTimer != null) clearTimeout(this.readyFallbackTimer)
    this.readyFallbackTimer = null
    await this.rpc.command('/logout')
  }

  async loginWithIdentity(identity: AuthIdentity, defaultOnError?: boolean, guest?: boolean): Promise<void> {
    const flag = (defaultOnError === true ? ' --default-on-error' : '') + (guest === true ? ' --guest' : '')
    await this.rpc.command(`/login_identity ${encodeIdentity(identity)}${flag}`)
    this.scheduleReadyFallback()
  }

  // "Jump in": reuse the SSO identity via `/login_identity`; if none is stored, fall back to
  // the engine's own saved login.
  async jumpIn(defaultOnError?: boolean): Promise<void> {
    const login = await getLogin()
    if (login) await this.loginWithIdentity(login.identity, defaultOnError, login.guest === true)
    else await this.loginPrevious(defaultOnError)
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
    if (this.readyFallbackTimer != null) clearTimeout(this.readyFallbackTimer)
    this.ch.close()
    this.listeners.clear()
  }

  renderBusy(): boolean {
    return this.rpc.renderBusy()
  }

  engineReady(): boolean {
    // Ready-to-launch (WASM compiled + GPU warm), not console-ready — the console only comes up after
    // launch.
    return this.rpc.readyToLaunch()
  }

  loadProgress(): number {
    return this.rpc.loadProgress()
  }

  loadStep(): string | null {
    return this.rpc.loadStep()
  }

  enginePanic(): { message: string } | null {
    return this.rpc.enginePanic()
  }

  clearEnginePanic(): void {
    this.rpc.clearEnginePanic()
  }

  rearmCrashWatchdog(): void {
    this.rpc.rearmCrashWatchdog()
  }

  launch(realm?: string, position?: string, host?: LaunchHostOptions): void {
    this.rpc.launch(realm, position, host)
  }

  homeScene(): { realm: string | null; parcel: string } | null {
    return this.rpc.homeScene()
  }

  termsAccepted(): boolean | null {
    return this.rpc.termsAccepted()
  }

  command(line: string): Promise<string> {
    return this.rpc.command(line)
  }

  private emit(msg: SceneToPage): void {
    this.listeners.forEach((fn) => fn(msg))
  }

  // The bridge scene emits a precise playerReady; until it does, hand off to the
  // engine a few seconds after login so the world isn't hidden forever.
  private scheduleReadyFallback(): void {
    if (this.readyFallbackTimer != null) clearTimeout(this.readyFallbackTimer)
    this.readyFallbackTimer = setTimeout(() => {
      this.readyFallbackTimer = null
      if (!this.playerReadyFired) {
        this.playerReadyFired = true
        this.emit({ kind: 'event', name: 'playerReady' })
      }
    }, 6000)
  }
}
