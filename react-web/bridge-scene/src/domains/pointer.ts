// Pointer / hover relay: the engine's hover stream → React, which draws the reticle + the
// "press E to interact" prompt (screen-space, so it lives in the DOM HUD, not here). Ported from
// the SDK7 bevy-ui-scene `components/hover-actions`; the bridge just forwards the relevant actions.
import { PointerEventType, PointerLock, engine } from '@dcl/sdk/ecs'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'
import type { HoverAction } from '../../../src/engine/protocol'

const TARGET_UI = 1 // HoverTargetType.UI — ignore hovers over engine UI

export function registerPointer(ctx: Ctx): void {
  // Cursor-lock → crosshair. The engine writes PbPointerLock.isPointerLocked to the CAMERA entity
  // whenever it grabs the mouse for camera-look (first OR third person); React draws the center
  // crosshair then. (bevy doesn't use the browser Pointer Lock API, so the page can't detect this
  // itself — and PrimaryPointerInfo.screenCoordinates is NOT null when locked, it's the center.)
  let lastLocked: boolean | null = null
  ctx.push(() => {
    const locked = PointerLock.getOrNull(engine.CameraEntity)?.isPointerLocked === true
    if (locked !== lastLocked) {
      lastLocked = locked
      ctx.send({ kind: 'cursorLock', locked })
    }
  })

  void (async () => {
    try {
      const stream = await BevyApi.getHoverStream()
      for await (const ev of stream) {
        if (!ev.entered || ev.targetType === TARGET_UI) {
          ctx.send({ kind: 'hover', actions: [] })
          continue
        }
        const actions: HoverAction[] = ev.actions
          .filter((a) => a.eventType === PointerEventType.PET_DOWN && a.eventInfo?.showFeedback !== false)
          .slice(0, 7)
          .map((a) => ({
            button: a.eventInfo?.button ?? 1,
            text: a.eventInfo?.hoverText ?? 'Interact',
            enabled: a.enabled !== false
          }))
        ctx.send({ kind: 'hover', actions })
      }
    } catch (e) {
      console.error('[pointer] hover stream failed', e)
    }
  })()
}
