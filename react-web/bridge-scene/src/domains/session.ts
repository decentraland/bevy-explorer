// Session: login RPC, scene-loading stream, player-ready, sidebar nav.
//   from: BevyApi (login + getSceneLoadingUIStream), @dcl/sdk getPlayer (player-ready).
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

export function registerSession(ctx: Ctx): void {
  // Login surface (request/response by method). Most clients log in via the engine's
  // `/login_identity` console command now; this stays for channel-based callers.
  ctx.on('rpc:req', async (msg) => {
    try {
      let value: unknown
      switch (msg.method) {
        case 'getPreviousLogin': value = await BevyApi.getPreviousLogin(); break
        case 'loginPrevious': value = await BevyApi.loginPrevious(); break
        // The engine has no "log in with a raw identity" surface; reuse the saved login instead.
        case 'loginIdentity': value = await BevyApi.loginPrevious(); break
        case 'loginGuest': BevyApi.loginGuest(); break
        case 'loginCancel': BevyApi.loginCancel(); break
        case 'logout': BevyApi.logout(); break
        default: throw new Error(`unsupported method ${String(msg.method)}`)
      }
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
  void (async () => {
    try {
      const stream = await BevyApi.getSceneLoadingUIStream()
      for await (const s of stream) {
        ctx.send({
          kind: 'sceneLoading',
          state: {
            visible: s.visible === true,
            realmConnected: s.realmConnected !== false,
            title: s.title ?? '',
            pendingAssets: s.pendingAssets ?? null
          }
        })
      }
    } catch (e) {
      console.error('[session] sceneLoading relay failed', e)
    }
  })()

  // Player-spawned signal (one-shot).
  let ready = false
  ctx.push(() => {
    if (ready) return
    if (getPlayer() != null) {
      ready = true
      ctx.send({ kind: 'event', name: 'playerReady' })
    }
  })
}
