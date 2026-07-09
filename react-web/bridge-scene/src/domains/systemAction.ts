// Relays HUD-relevant engine system actions to React. Today only 'Cancel' (bound to Escape) — the HUD
// uses it to close the topmost popup. Read authoritatively from the engine's input stream so it fires
// even while the engine holds keyboard focus (a plain DOM keydown wouldn't see it).
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

export function registerSystemAction(ctx: Ctx): void {
  void (async () => {
    try {
      const stream = await BevyApi.getSystemActionStream()
      for await (const ev of stream) {
        if (ev.action === 'Cancel' && ev.pressed) {
          ctx.send({ kind: 'systemAction', action: 'Cancel' })
        }
      }
    } catch (e) {
      console.error('[systemAction] stream failed', e)
    }
  })()
}
