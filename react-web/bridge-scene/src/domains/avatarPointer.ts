// Nearby-avatar click → profile card. Ported from bevy-ui-scene's avatar-tracker: nearby avatars are
// ECS entities carrying PlayerIdentityData (spawned by the engine, hold the address). We attach a
// standard SDK7 pointer-down to each one (hover "Show Profile") and forward the click to React, which
// opens the ProfileCard anchored at the cursor. No BevyApi needed for the click — plain PointerEvents.
import { engine, pointerEventsSystem, InputAction, PlayerIdentityData, PointerLock, PrimaryPointerInfo, type Entity } from '@dcl/sdk/ecs'
import { getPlayer } from '@dcl/sdk/players'
import type { Ctx } from '../bridge'

export function registerAvatarPointer(ctx: Ctx): void {
  const registered = new Set<Entity>()
  let frame = 0
  ctx.push(() => {
    // Avatars enter/leave constantly; rescan ~2×/s (PlayerIdentityData is the engine's source of
    // truth, same as nametags) and diff against what already has a handler.
    if (frame++ % 30 !== 0) return
    const present = new Set<Entity>()
    for (const [entity, data] of engine.getEntitiesWith(PlayerIdentityData)) {
      if (data.address === '') continue
      present.add(entity)
      if (registered.has(entity)) continue
      registered.add(entity)
      pointerEventsSystem.onPointerDown(
        { entity, opts: { button: InputAction.IA_POINTER, hoverText: 'Show Profile' } },
        () => {
          const address = PlayerIdentityData.getOrNull(entity)?.address
          if (address == null || address === '') return
          // Cursor screen position at the click — where React anchors the profile card.
          const p = PrimaryPointerInfo.getOrNull(engine.RootEntity)?.screenCoordinates
          // The engine grabs the cursor for camera-look; free it so the card is usable.
          const pl = PointerLock.getMutableOrNull(engine.CameraEntity)
          if (pl != null) pl.isPointerLocked = false
          ctx.send({
            kind: 'avatarClick',
            address,
            name: getPlayer({ userId: address })?.name ?? address,
            x: p?.x ?? 0,
            y: p?.y ?? 0
          })
        }
      )
    }
    for (const entity of registered) {
      if (present.has(entity)) continue
      registered.delete(entity)
      pointerEventsSystem.removeOnPointerDown(entity)
    }
  })
}
