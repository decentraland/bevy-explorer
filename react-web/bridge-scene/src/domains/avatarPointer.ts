// Nearby-avatar click → profile card. Ported from bevy-ui-scene's avatar-tracker: nearby avatars are
// ECS entities carrying PlayerIdentityData (spawned by the engine, hold the address). We attach a
// standard SDK7 pointer-down to each one (hover "Show Profile") and forward the click to React, which
// opens the ProfileCard anchored at the cursor.
import { engine, pointerEventsSystem, InputAction, PlayerIdentityData, PointerLock, PrimaryPointerInfo, type Entity } from '@dcl/sdk/ecs'
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'
import { throttleByDt, singleFlight } from '../system-helpers'

export function registerAvatarPointer(ctx: Ctx): void {
  const registered = new Set<Entity>()
  // Addresses (lowercased) currently hide_profile per an AvatarModifierArea's DISABLE_PASSPORTS
  // (the old scene's avatar-tracker — isProfileBlocked — respected this too), or hide per
  // HIDE_AVATARS — the engine only hides the mesh (the collider stays live), so without this an
  // invisible avatar would still show "Show Profile" over empty space and leak who's there. A
  // disabled avatar is treated as absent in the rescan, so its pointer-down (and the hover) is
  // removed within a poll cycle; the click-time check stays as a belt-and-braces for that stale window.
  let passportDisabled = new Set<string>()

  // Two independent dt-throttled systems, mirroring bevy-ui-scene's avatar-tracker (two separate
  // engine.addSystem timers): the privacy-modifier RPC is polled on its own slow beat (single-flight
  // so a slow engine can't stack overlapping calls), while the local entity rescan runs more often.
  ctx.push(
    throttleByDt(
      1.0,
      singleFlight(async () => {
        const mods = await BevyApi.getAvatarModifiers().catch(() => [])
        passportDisabled = new Set(
          mods.filter((m) => m.hideProfile || m.hideAvatar).map((m) => m.userId.toLowerCase())
        )
      })
    )
  )

  // Avatars enter/leave constantly; rescan ~2×/s (PlayerIdentityData is the engine's source of
  // truth, same as nametags) and diff against what already has a handler.
  ctx.push(
    throttleByDt(0.5, () => {
      // Never attach our own avatar — PlayerIdentityData is written for the local player too (profile
      // sync), unlike most foreign-only ECS data.
      const me = getPlayer()?.userId?.toLowerCase()
      const present = new Set<Entity>()
      for (const [entity, data] of engine.getEntitiesWith(PlayerIdentityData)) {
        if (data.address === '' || data.address.toLowerCase() === me) continue
        if (passportDisabled.has(data.address.toLowerCase())) continue
        present.add(entity)
        if (registered.has(entity)) continue
        registered.add(entity)
        pointerEventsSystem.onPointerDown(
          { entity, opts: { button: InputAction.IA_POINTER, hoverText: 'Show Profile' } },
          () => {
            const address = PlayerIdentityData.getOrNull(entity)?.address
            if (address == null || address === '') return
            if (passportDisabled.has(address.toLowerCase())) return // privacy area: passports disabled
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
  )
}
