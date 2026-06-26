// World-space avatar nametags — a billboarded name label pinned above each player's head.
// This is inherently in-engine (the React DOM HUD is screen-space and can't pin/billboard to a
// moving 3D head). Ported from the SDK7 bevy-ui-scene `components/avatar-tags`, but the content is
// rewritten clean: the original pulled in the old scene's redux store + sprite atlas + fontsize +
// voice/chat plumbing, none of which the headless bridge-scene has. Here it's just the name.
import ReactEcs, { UiEntity } from '@dcl/react-ecs'
import {
  AvatarAnchorPointType,
  AvatarAttach,
  Billboard,
  CameraMode,
  CameraType,
  Material,
  MaterialTransparencyMode,
  MeshRenderer,
  PlayerIdentityData,
  Transform,
  UiCanvas,
  engine
} from '@dcl/sdk/ecs'
import { Color4, Vector3 } from '@dcl/sdk/math'
import { getPlayer, onEnterScene, onLeaveScene } from '@dcl/sdk/players'
import { ReactEcsRenderer } from '@dcl/sdk/react-ecs'
import type { Entity } from '@dcl/ecs'

// The label UI rendered into each tag's UiCanvas → texture. Re-evaluated by ReactEcs, so the name
// stays current and the own-avatar tag hides itself in first-person (where you can't see it anyway).
function tagElement(userId: string): () => ReactEcs.JSX.Element | null {
  return function Tag(): ReactEcs.JSX.Element | null {
    const name = getPlayer({ userId })?.name
    if (name == null || name === '') return null
    const isSelf = userId === getPlayer()?.userId
    const firstPerson = CameraMode.get(engine.CameraEntity).mode === CameraType.CT_FIRST_PERSON
    if (isSelf && firstPerson) return null
    return (
      <UiEntity uiTransform={{ width: '100%', justifyContent: 'center', alignItems: 'center' }}>
        <UiEntity
          uiTransform={{ padding: { top: 4, bottom: 4, left: 12, right: 12 }, borderRadius: 14 }}
          uiBackground={{ color: Color4.create(0, 0, 0, 0.5) }}
          uiText={{
            value: `<b>${name}</b>`,
            fontSize: 18,
            color: Color4.White(),
            textAlign: 'middle-center',
            outlineColor: Color4.Black(),
            outlineWidth: 0.12
          }}
        />
      </UiEntity>
    )
  }
}

// A plane attached to the avatar's NAME_TAG anchor, billboarded to the camera, textured with the
// label's UiCanvas (alpha-blended + faintly emissive so it stays legible in any lighting).
function createTag(userId: string): Entity {
  const tag = engine.addEntity()
  Billboard.create(tag, {})
  MeshRenderer.setPlane(tag)
  Transform.create(tag, { scale: Vector3.create(2, 1, 1) })
  AvatarAttach.create(tag, { avatarId: userId, anchorPointId: AvatarAnchorPointType.AAPT_NAME_TAG })
  UiCanvas.create(tag, { width: 400, height: 200, color: Color4.Clear() })
  ReactEcsRenderer.setTextureRenderer(tag, tagElement(userId))
  const uiTexture = { tex: { $case: 'uiTexture' as const, uiTexture: { uiCanvasEntity: tag } } }
  Material.setPbrMaterial(tag, {
    transparencyMode: MaterialTransparencyMode.MTM_ALPHA_BLEND,
    texture: uiTexture,
    emissiveTexture: uiTexture,
    emissiveColor: Color4.White(),
    emissiveIntensity: 0.2
  })
  return tag
}

export function initNametags(): void {
  const tags = new Map<string, Entity>()
  const add = (userId: string): void => {
    if (tags.has(userId)) return
    tags.set(userId, createTag(userId))
  }
  const remove = (userId: string): void => {
    const e = tags.get(userId)
    if (e == null) return
    engine.removeEntityWithChildren(e)
    tags.delete(userId)
  }

  // Players already in the scene at startup, plus our own avatar, then keep it live.
  for (const [, data] of engine.getEntitiesWith(PlayerIdentityData)) add(data.address)
  const self = getPlayer()?.userId
  if (self != null) add(self)
  onEnterScene((player) => add(player.userId))
  onLeaveScene((userId) => remove(userId))
}
