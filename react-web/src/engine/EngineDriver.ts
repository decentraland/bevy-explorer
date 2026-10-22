// Real engine session driver.
//   - Login ACTIONS go over engine-native console commands (no scene needed):
//     /login_guest, /login_previous, /logout  (agent_commands.rs)
//   - STREAMS/events (scene loading, player-ready, chat, chat visibility) arrive
//     from the super-user bridge scene over the `bevy-ui-bridge` BroadcastChannel
//     and are delivered through ONE generic `on(msg => …)` subscription.

import { encodeIdentity, getLogin, rootAddress, type AuthIdentity, type StoredLogin } from '../features/auth/sso'
import { trackGuestWalletCreation } from '../features/auth/guestWalletEvent'
import { attachEngine, launchAnalytics } from '../lib/analytics'
import { serviceUrl } from '../lib/baseDomain'
import { inShell, shellRequest } from '../lib/shell'
import { newAccountFlags, type LoginDriver, type NewAccountPick } from './driver'
import type { EngineRpc, LaunchHostOptions } from './engineRpc'
import { BridgeChannel } from './bridgeChannel'
import { bridgeChannelName, type PageToScene, type SceneToPage } from './protocol'

// Two thirdweb round-trips.
const CREATE_GUEST_TIMEOUT_MS = 30_000

export class EngineDriver implements LoginDriver {
  private readonly ch: BridgeChannel
  private readonly listeners = new Set<(msg: SceneToPage) => void>()

  constructor(private readonly rpc: EngineRpc) {
    this.ch = new BridgeChannel(
      bridgeChannelName(),
      (msg) => {
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
    return this.rpc.command(`/login_previous${flag}`)
  }

  async loginGuest(): Promise<void> {
    await this.rpc.command('/login_guest')
  }

  // The shell creates the guest account and keeps it, key included, like a sign-in; the engine
  // gets the public identity and signs through the shell. Outside the shell (a credentialless
  // embed) there is nowhere to keep it, so the guest is a throwaway one.
  async loginPersistentGuest(pick?: NewAccountPick): Promise<void> {
    if (!inShell) return this.loginGuest()
    const login = await trackGuestWalletCreation(() => shellRequest<StoredLogin>('createGuest', undefined, CREATE_GUEST_TIMEOUT_MS))
    await this.loginWithIdentity(login.identity, false, true, pick)
  }

  async loginCancel(): Promise<void> {
    // SSO login is a redirect/console-command; there is no in-engine flow to cancel.
  }

  async logout(): Promise<void> {
    await this.rpc.command('/logout')
  }

  // The profile is fetched from this page's catalyst: the one the auth site deploys a new
  // account's to, and the one the page asks before signing in (useStoredProfile), rather than the
  // realm's node, which can lag behind it.
  async loginWithIdentity(identity: AuthIdentity, defaultOnError?: boolean, guest?: boolean, pick?: NewAccountPick): Promise<void> {
    const flag =
      (defaultOnError === true ? ' --default-on-error' : '') +
      (guest === true ? ' --guest' : '') +
      newAccountFlags(pick) +
      ` --profiles ${serviceUrl('catalyst')}/lambdas`
    await this.rpc.command(`/login_identity ${encodeIdentity(identity)}${flag}`)
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
    // the page's analytics session and campaign id ride along so engine events carry them too,
    // and from here on the page's events leave through the engine's console
    this.rpc.launch(realm, position, { ...host, ...launchAnalytics() })
    attachEngine((line) => this.rpc.command(line))
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
}
