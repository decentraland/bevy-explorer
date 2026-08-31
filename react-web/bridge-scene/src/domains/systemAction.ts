// Relays HUD-relevant engine system actions to React (menu/panel hotkeys, quick emotes, Cancel,
// and the Scroll edges that drive HUD panel scrolling from non-wheel bindings).
// Read authoritatively from the engine's input stream so it fires even while the engine holds
// keyboard focus (a plain DOM keydown wouldn't see it), and so user rebinds are respected.
// The camera/pointer axis families are not forwarded — they edge repeatedly during normal play
// (mouse motion, gamepad sticks) and the HUD has no use for them. Scroll IS forwarded: the HUD
// scrolls the hovered scrollable panel on its press/release edges (see useEngineSession).
//
// This is the scene's ONLY consumer of the stream: the engine keys the receiver by type in a single
// slot and ignores the rid, so a second `getSystemActionStream()` consumer would steal the receiver
// and silently end one of the two `for await` loops. Other domains subscribe via `onSystemAction`.
import { BevyApi } from '../bevy-api'
import type { SystemActionEvent } from '../bevy-api'
import type { Ctx } from '../bridge'

const AXIS_ACTION = /^(Camera|Pointer)(Zoom)?(In|Out|Up|Down|Left|Right)$/

const listeners = new Set<(ev: SystemActionEvent) => void>()

// Subscribe to engine system actions. Call at registration time — the stream opens once, here.
export function onSystemAction(fn: (ev: SystemActionEvent) => void): void {
  listeners.add(fn)
}

export function registerSystemAction(ctx: Ctx): void {
  void (async () => {
    try {
      const stream = await BevyApi.getSystemActionStream()
      for await (const ev of stream) {
        if (!AXIS_ACTION.test(ev.action)) {
          ctx.send({ kind: 'systemAction', action: ev.action, pressed: ev.pressed })
        }
        for (const fn of listeners) {
          try {
            fn(ev)
          } catch (e) {
            console.error('[systemAction] listener failed', e)
          }
        }
      }
    } catch (e) {
      console.error('[systemAction] stream failed', e)
    }
  })()
}
