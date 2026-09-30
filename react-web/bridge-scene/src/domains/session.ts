// Session: login RPC, scene-loading stream, player-ready, sidebar nav.
//   from: BevyApi (login + getSceneLoadingUIStream), @dcl/sdk getPlayer (player-ready).
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import { relay } from '../system-helpers'
import { identity } from '../identity'
import type { Ctx } from '../bridge'

const SIGN_INS = new Set(['loginPrevious', 'loginNew', 'loginIdentity'])

export function registerSession(ctx: Ctx): void {
  // Player-spawned signal: one-shot per page, not per scene. The page gates its world-entry
  // fetches on it and never retries, so a page that arrives late is re-told on `hello`.
  let ready = false

  // Login surface (request/response by method). Most clients log in via the engine's
  // `/login_identity` console command now; this stays for channel-based callers.
  ctx.on('rpc:req', async (msg) => {
    try {
      let value: unknown
      switch (msg.method) {
        case 'getPreviousLogin': value = await BevyApi.getPreviousLogin(); break
        case 'loginPrevious': value = await BevyApi.loginPrevious(); break
        // Fresh sign-in (remote wallet): forward the mid-flight verification code as its own
        // message; the rpc itself resolves on approval (rejects on failure/cancel via catch).
        case 'loginNew': {
          const login = BevyApi.loginNew()
          void login.code
            .then((code) => { ctx.send({ kind: 'loginCode', code: code ?? null }); })
            .catch(() => undefined) // errors surface through `success` below
          await login.success
          value = { success: true, error: '' }
          break
        }
        // The engine has no "log in with a raw identity" surface; reuse the saved login instead.
        case 'loginIdentity': value = await BevyApi.loginPrevious(); break
        case 'loginGuest': BevyApi.loginGuest(); break
        case 'loginCancel': BevyApi.loginCancel(); break
        case 'logout': BevyApi.logout(); break
        default: throw new Error(`unsupported method ${String(msg.method)}`)
      }
      // The engine keeps the old identity through a logout, so a completed sign-in is what says the
      // (possibly same) account is back and must be announced again.
      if (SIGN_INS.has(msg.method) && (value as { success?: boolean } | undefined)?.success !== false) ready = false
      ctx.send({ kind: 'rpc:res', id: msg.id, ok: true, value })
    } catch (err) {
      ctx.send({ kind: 'rpc:res', id: msg.id, ok: false, error: String(err) })
    }
  })

  // Sidebar nav: every panel is rendered by React now; only the engine-side mic toggle
  // is handled here (the rest are no-ops).
  ctx.on('navAction', (msg) => {
    if (msg.action === 'mic') {
      void BevyApi.getMicState().then((m) => {
        BevyApi.setMicEnabled(!m.enabled)
      })
    }
  })

  // Scene-asset loading stream → React loading screen.
  relay('sceneLoading', async () => await BevyApi.getSceneLoadingUIStream(), (s) => {
    ctx.send({
      kind: 'sceneLoading',
      state: {
        visible: s.visible,
        realmConnected: s.realmConnected,
        title: s.title ?? '',
        pendingAssets: s.pendingAssets ?? null
      }
    })
  })

  identity.onChange(() => {
    ready = false
  })
  ctx.push(() => {
    const player = getPlayer()
    identity.observe(player?.userId ?? null)
    if (ready) return
    if (player != null) {
      ready = true
      ctx.send({ kind: 'event', name: 'playerReady' })
    }
  })
  ctx.on('hello', () => {
    if (ready) ctx.send({ kind: 'event', name: 'playerReady' })
  })
}
