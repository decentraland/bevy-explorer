// World: current parcel + teleport, and the mic state.
//   from: @dcl/sdk getPlayer().position (parcel), RestrictedActions.teleportTo,
//         BevyApi.getMicState() / setMicEnabled().
import { getPlayer } from '@dcl/sdk/players'
import { teleportTo, changeRealm } from '~system/RestrictedActions'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

// Echo a "DCL System" line into the React chat (empty sender → system member). Used to relay
// slash-command feedback (/commands output, /reload status) that isn't broadcast to other players.
function pushSystem(ctx: Ctx, message: string): void {
  ctx.send({ kind: 'chat', chat: { sender: '', message, channel: 'Nearby' } })
}

export function registerWorld(ctx: Ctx): void {
  ctx.on('getMap', () => {
    const pos = getPlayer()?.position
    ctx.send({ kind: 'mapState', x: Math.floor((pos?.x ?? 0) / 16), y: Math.floor((pos?.z ?? 0) / 16) })
  })

  ctx.on('teleport', (msg) => {
    teleportTo({ worldCoordinates: { x: msg.x, y: msg.y } }).catch((e: unknown) => {
      console.error('[world] teleport failed', e)
    })
  })

  // Travel to a world/realm (e.g. boedo.dcl.eth). The engine auto-grants ChangeRealm for our
  // super-user scene, so the React HUD owns the confirmation prompt.
  ctx.on('changeRealm', (msg) => {
    changeRealm({ realm: msg.realm }).catch((e: unknown) => {
      console.error('[world] changeRealm failed', e)
    })
  })

  // `/reload` — reload the scene the player is standing in, resolved by parcel from liveSceneInfo.
  // Never the super-user bridge (isSuper filtered out) and never reload-all, so the HUD survives.
  ctx.on('reloadScene', () => {
    const op = BevyApi.consoleCommand
    if (op == null) {
      pushSystem(ctx, 'Reload is not available.')
      return
    }
    const pos = getPlayer()?.position
    const px = Math.floor((pos?.x ?? 0) / 16)
    const py = Math.floor((pos?.z ?? 0) / 16)
    BevyApi.liveSceneInfo()
      .then((scenes) => {
        const current = scenes.find((s) => s.isSuper !== true && (s.parcels ?? []).some((p) => p.x === px && p.y === py))
        if (current == null) {
          pushSystem(ctx, 'Could not find the current scene to reload.')
          return
        }
        return op('reload', [current.hash]).then(() => pushSystem(ctx, `Reloading ${current.title || current.hash}…`))
      })
      .catch((e: unknown) => {
        console.error('[world] reload failed', e)
        pushSystem(ctx, 'Reload failed.')
      })
  })

  // `/commands` — surface the engine console's own command list. Run its `help`; if `help` isn't a
  // registered command the engine rejects with "Recognized commands: [...]" — exactly the list we
  // want — so relay either the successful output or the rejection text.
  ctx.on('consoleCommand', (msg) => {
    const op = BevyApi.consoleCommand
    if (op == null) {
      pushSystem(ctx, 'Engine console is not available.')
      return
    }
    op(msg.command, msg.args ?? [])
      .then((out) => pushSystem(ctx, out.trim() || `(no output for ${msg.command})`))
      .catch((e: unknown) => pushSystem(ctx, e instanceof Error ? e.message : String(e)))
  })

  ctx.on('setMic', (msg) => {
    BevyApi.setMicEnabled(msg.enabled)
  })

  // Mic state → React mic toggle. Poll ~1s, push on change.
  let acc = 1
  let lastKey = ''
  ctx.push((dt) => {
    acc += dt
    if (acc < 1) return
    acc = 0
    BevyApi.getMicState()
      .then((s) => {
        const key = `${String(s.enabled)}|${String(s.available)}`
        if (key === lastKey) return
        lastKey = key
        ctx.send({ kind: 'mic', enabled: s.enabled, available: s.available })
      })
      .catch(() => undefined)
  })
}
