// Proximity tooltips: the engine reports in-range world entities; we project each entity's world
// position to SCREEN pixels every frame (the hard part the DOM can't do) and relay the coords +
// actions to React, which renders the same black-pill chips as the hover prompt anchored on them.
import { PointerEventType, Transform, UiCanvasInformation, engine } from '@dcl/sdk/ecs'
import { BevyApi, type HoverEntry, type Vec3 } from '../bevy-api'
import { relay } from '../system-helpers'
import type { Ctx } from '../bridge'
import type { HoverAction, ProximityTip } from '../../../src/engine/protocol'
import { projectToScreen, createFovTracker, type Quat } from './project'
import { proximityKey } from '../../../src/engine/pointerKeys'

function toActions(entries: HoverEntry[]): HoverAction[] {
  return entries
    .filter((a) => a.eventType === PointerEventType.PET_DOWN && a.enabled && a.eventInfo?.showFeedback !== false)
    .slice(0, 4)
    .map((a) => ({ button: a.eventInfo?.button ?? 1, text: a.eventInfo?.hoverText ?? 'Interact', enabled: true }))
}

export function registerProximity(ctx: Ctx): void {
  const inRange = new Map<number, { pos: Vec3; actions: HoverAction[] }>()
  const fov = createFovTracker()

  // A reopened stream reports only new enters and exits, so the old set is dropped first.
  const open = async (): Promise<Awaited<ReturnType<typeof BevyApi.getProximityStream>>> => {
    inRange.clear()
    return await BevyApi.getProximityStream()
  }
  relay('proximity', open, (ev) => {
    if (ev.entered) inRange.set(ev.entity, { pos: ev.entityPosition, actions: toActions(ev.actions) })
    else inRange.delete(ev.entity)
  })

  // Sent only when a tip appears, goes or moves by a whole pixel, not every frame.
  let lastKey = ''
  const send = (tips: ProximityTip[]): void => {
    const key = proximityKey(tips)
    if (key === lastKey) return
    lastKey = key
    ctx.send({ kind: 'proximity', tips })
  }
  ctx.push((dt) => {
    fov.tick(dt)
    if (inRange.size === 0) {
      send([])
      return
    }
    const camT = Transform.getOrNull(engine.CameraEntity)
    const canvas = UiCanvasInformation.getOrNull(engine.RootEntity)
    if (camT == null || canvas == null || canvas.width <= 0 || canvas.height <= 0) return
    const tips: ProximityTip[] = []
    for (const [id, e] of inRange) {
      if (e.actions.length === 0) continue
      const p = projectToScreen(e.pos, camT.position, camT.rotation as Quat, fov.fovY(), canvas.width, canvas.height)
      if (p != null) tips.push({ id, x: p.x, y: p.y, actions: e.actions })
    }
    send(tips)
  })
}
