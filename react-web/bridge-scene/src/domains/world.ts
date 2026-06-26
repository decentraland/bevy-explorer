// World: current parcel + teleport, and the mic state.
//   from: @dcl/sdk getPlayer().position (parcel), RestrictedActions.teleportTo,
//         BevyApi.getMicState() / setMicEnabled().
import { getPlayer } from '@dcl/sdk/players'
import { teleportTo, changeRealm } from '~system/RestrictedActions'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

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
