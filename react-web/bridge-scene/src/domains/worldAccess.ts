// World access: relays the engine's password prompt for a password-protected world the player is
// travelling to (ipfs::world_access) to the React HUD, and the answer back. A refused answer comes
// back as a new prompt carrying the reason; the prompt is withdrawn when its realm change is over.
//   from: SystemApi.getWorldAccessStream / setWorldPassword.
import { BevyApi } from '../bevy-api'
import { relay } from '../system-helpers'
import type { Ctx } from '../bridge'

export function registerWorldAccess(ctx: Ctx): void {
  const open = BevyApi.getWorldAccessStream
  if (open == null) return
  relay('worldAccess', async () => await open(), (ev) => {
    if (ev.type === 'cancelled') {
      ctx.send({ kind: 'worldPasswordWithdrawn' })
      return
    }
    ctx.send({ kind: 'worldPasswordRequest', world: ev.world, error: ev.error ?? undefined })
  })

  ctx.on('worldPasswordResolve', (msg) => {
    BevyApi.setWorldPassword?.({ password: msg.password })
  })
}
