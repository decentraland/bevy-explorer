// Pointer / hover relay: the engine's hover stream → React, which draws the reticle + the
// "press E to interact" prompt (screen-space, so it lives in the DOM HUD, not here). Ported from
// the SDK7 bevy-ui-scene `components/hover-actions`; the bridge just forwards the relevant actions.
import { PointerEventType, PointerLock, PrimaryPointerInfo, engine } from '@dcl/sdk/ecs'
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
  // The hover stream only fires on enter/exit, so the cursor position captured there goes stale as the
  // mouse keeps moving over the same entity. While a hover is active and the cursor is free, stream the
  // live position each frame (only when it actually moves) so the tooltip follows the pointer.
  let hoverActive = false
  let lastX = -1
  let lastY = -1
  ctx.push(() => {
    const locked = PointerLock.getOrNull(engine.CameraEntity)?.isPointerLocked === true
    if (locked !== lastLocked) {
      lastLocked = locked
      ctx.send({ kind: 'cursorLock', locked })
    }
    if (hoverActive && !locked) {
      const p = PrimaryPointerInfo.getOrNull(engine.RootEntity)?.screenCoordinates
      if (p != null && (p.x !== lastX || p.y !== lastY)) {
        lastX = p.x
        lastY = p.y
        ctx.send({ kind: 'hoverPos', x: p.x, y: p.y })
      }
    }
  })

  void (async () => {
    try {
      const stream = await BevyApi.getHoverStream()
      for await (const ev of stream) {
        if (!ev.entered || ev.targetType === TARGET_UI) {
          hoverActive = false
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
        // Cursor screen position so React anchors the hint at the pointer (centre while locked).
        const p = PrimaryPointerInfo.getOrNull(engine.RootEntity)?.screenCoordinates
        hoverActive = actions.length > 0
        ctx.send({ kind: 'hover', actions, x: p?.x, y: p?.y })
      }
    } catch (e) {
      console.error('[pointer] hover stream failed', e)
    }
  })()
}
