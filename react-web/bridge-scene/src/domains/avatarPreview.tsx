// Avatar preview: the live 3D avatar shown in the React Backpack's left column.
//
// React carves a transparent hole (EngineViewport region='avatarPreview') and sends its
// screen rect over the bridge. We render the player's avatar with a dedicated TextureCamera
// (its own CameraLayer, so no world/skybox/other avatars) into a UI videoTexture positioned
// at that rect — the engine composites it behind the React DOM, showing through the hole.
//
// The preview reads getPlayer()'s avatar, which React updates live via setAvatar (equip), so
// equipping in the Backpack reflects here. Mounted via ReactEcsRenderer in index.ts.
import ReactEcs, { UiEntity } from '@dcl/react-ecs'
import { AvatarShape, CameraLayer, CameraLayers, Material, MeshRenderer, PrimaryPointerInfo, TextureCamera, Transform, engine } from '@dcl/sdk/ecs'
import { Color4, Quaternion, Vector3 } from '@dcl/sdk/math'
import { getPlayer } from '@dcl/sdk/players'
import type { Entity } from '@dcl/ecs'
import type { Ctx } from '../bridge'

type Rect = { x: number; y: number; width: number; height: number }

const LAYER = 10
// Menu purple backdrop (two-tone: lighter top, darker bottom), matched to Unity's vivid
// magenta→purple menu gradient so the avatar cutout blends with the React panels around it
// (the avatar column sits on the darker-left side of that radial).
const BACKDROP_TOP = Color4.create(0.439, 0.086, 0.659, 1)
const BACKDROP_BOTTOM = Color4.create(0.329, 0.035, 0.541, 1)
// Gold/orange podium the avatar stands on (Unity's CharacterPreview platform).
const PODIUM_COLOR = Color4.create(0.95, 0.62, 0.18, 1)
const PODIUM_EMISSIVE = Color4.create(0.85, 0.42, 0.08, 1)
// Body-shot framing (from the SDK7 backpack): camera in front of the avatar, orthographic.
// Vertical range tuned so the whole avatar fits head-to-toe with a little margin top and
// bottom (a tighter range clipped the top of the head / the feet).
const VERTICAL_RANGE = 5.5
// Drag-to-rotate sensitivity (from the SDK7 AvatarPreviewElement).
const ROTATION_FACTOR = -0.5

let rect: Rect | null = null
let avatarEntity: Entity | null = null
let cameraEntity: Entity | null = null
let podiumEntity: Entity | null = null
let lastShapeKey = ''
// Non-persisting preview override (selecting an item in the Backpack): when set, the preview
// avatar wears these urns instead of the player's actual equipped set. null = no override.
let previewUrns: string[] | null = null

// TextureCamera resolution matching the cutout's aspect ratio, so a 1:1 stretch into the
// rect never distorts the avatar (the column is tall/narrow; a square render would squash it).
function camRes(r: Rect): { width: number; height: number } {
  const h = 1600
  const aspect = r.height > 0 ? r.width / r.height : 0.55
  return { width: Math.max(64, Math.round(h * aspect)), height: h }
}

function avatarShape(): {
  id: string
  bodyShape?: string
  eyeColor?: { r: number; g: number; b: number }
  hairColor?: { r: number; g: number; b: number }
  skinColor?: { r: number; g: number; b: number }
  wearables: string[]
  emotes: string[]
  forceRender: string[]
} {
  const p = getPlayer()
  // Preview override (item selected, not equipped) takes precedence over the player's set,
  // so selecting shows the look without persisting anything to the profile.
  const wearables = previewUrns ?? (p?.wearables ?? []).filter((w): w is string => typeof w === 'string')
  return {
    id: p?.userId ?? '',
    bodyShape: p?.avatar?.bodyShapeUrn,
    eyeColor: p?.avatar?.eyesColor,
    hairColor: p?.avatar?.hairColor,
    skinColor: p?.avatar?.skinColor,
    wearables,
    emotes: [],
    forceRender: p?.forceRender ?? []
  }
}

function createPreview(): void {
  if (avatarEntity != null || rect == null) return
  const res = camRes(rect)
  const a = engine.addEntity()
  const c = engine.addEntity()

  AvatarShape.create(a, { ...avatarShape(), name: undefined, talking: false })
  CameraLayers.create(a, { layers: [LAYER] })
  Transform.create(a, {
    position: Vector3.create(8, 0, 8),
    rotation: Quaternion.fromEulerDegrees(0, 180, 0),
    scale: Vector3.create(2, 2, 2)
  })

  // Gold podium under the avatar (a thin cylinder disc, preview-layer only) — matches
  // the platform the avatar stands on in Unity's backpack.
  const podium = engine.addEntity()
  MeshRenderer.setCylinder(podium, 1, 1)
  Material.setPbrMaterial(podium, {
    albedoColor: PODIUM_COLOR,
    emissiveColor: PODIUM_EMISSIVE,
    emissiveIntensity: 0.5,
    metallic: 0,
    roughness: 0.5
  })
  CameraLayers.create(podium, { layers: [LAYER] })
  Transform.create(podium, {
    position: Vector3.create(8, -0.06, 8),
    scale: Vector3.create(2.6, 0.12, 2.6)
  })

  CameraLayer.create(c, {
    layer: LAYER,
    directionalLight: false,
    showAvatars: false,
    showSkybox: false,
    showFog: false,
    ambientBrightnessOverride: 5
  })
  TextureCamera.create(c, {
    width: res.width,
    height: res.height,
    layer: LAYER,
    clearColor: Color4.create(0, 0, 0, 0),
    mode: { $case: 'orthographic', orthographic: { verticalRange: VERTICAL_RANGE } },
    volume: 1
  })
  Transform.create(c, {
    position: Vector3.create(8, 2.0, 0),
    rotation: Quaternion.fromEulerDegrees(4, 0, 0)
  })

  avatarEntity = a
  cameraEntity = c
  podiumEntity = podium
  lastShapeKey = ''
  syncShape()
}

function disposePreview(): void {
  if (avatarEntity != null) engine.removeEntity(avatarEntity)
  if (cameraEntity != null) engine.removeEntity(cameraEntity)
  if (podiumEntity != null) engine.removeEntity(podiumEntity)
  avatarEntity = null
  cameraEntity = null
  podiumEntity = null
}

// Re-read the player's avatar into the shape when it changes (equip via React → setAvatar).
function syncShape(): void {
  if (avatarEntity == null) return
  const shape = avatarShape()
  const key = `${shape.bodyShape ?? ''}|${shape.wearables.join(',')}`
  if (key === lastShapeKey) return
  lastShapeKey = key
  const mut = AvatarShape.getMutableOrNull(avatarEntity)
  if (mut != null) Object.assign(mut, shape)
}

export function registerAvatarPreview(ctx: Ctx): void {
  ctx.on('engineViewport', (msg) => {
    if (msg.region !== 'avatarPreview') return
    rect = msg.rect
    if (rect == null) {
      disposePreview()
      return
    }
    if (avatarEntity == null) createPreview()
    else if (cameraEntity != null) {
      // Window resized → keep the camera aspect matched to the new rect.
      const res = camRes(rect)
      const cam = TextureCamera.getMutableOrNull(cameraEntity)
      if (cam != null && (cam.width !== res.width || cam.height !== res.height)) {
        cam.width = res.width
        cam.height = res.height
      }
    }
  })

  // Selecting an item in the Backpack previews it on the avatar without persisting (null reverts).
  ctx.on('previewAvatar', (msg) => {
    previewUrns = msg.urns
    lastShapeKey = '' // force the next syncShape to apply the new (or reverted) set
    syncShape()
  })

  // Keep the preview in sync with the player. Poll fast until the avatar actually has a
  // body shape (the player may not be ready the instant the Backpack opens — that left the
  // column empty), then throttle to ~2/s to pick up equips.
  let acc = 0
  ctx.push((dt) => {
    if (rect == null || avatarEntity == null) return
    acc += dt
    const ready = lastShapeKey.length > 1
    if (acc < (ready ? 0.5 : 0.1)) return
    acc = 0
    syncShape()
  })
}

function rotateAvatar(): void {
  if (avatarEntity == null) return
  const pointer = PrimaryPointerInfo.getOrNull(engine.RootEntity)
  const deltaX = pointer?.screenDelta?.x ?? 0
  if (deltaX === 0) return
  const qY = Quaternion.fromAngleAxis(deltaX * ROTATION_FACTOR, Vector3.create(0, 1, 0))
  const cur = Transform.get(avatarEntity).rotation
  Transform.getMutable(avatarEntity).rotation = Quaternion.multiply(
    Quaternion.create(cur.x, cur.y, cur.z, cur.w),
    qY
  )
}

export function renderAvatarPreview(): ReactEcs.JSX.Element | null {
  if (rect == null || cameraEntity == null) return null
  const r = rect
  return (
    // Full-screen opaque base: the live world can never show through React's transparent
    // avatar cutout, even if the reported rect and the cutout don't line up to the pixel.
    // React paints everything except the avatar column, so this purple only ever shows
    // inside that column — it's a guaranteed backdrop, not a visible full-screen fill.
    <UiEntity
      uiTransform={{ positionType: 'absolute', position: { left: 0, top: 0 }, width: '100%', height: '100%' }}
      uiBackground={{ color: BACKDROP_BOTTOM }}
    >
      {/* Framed two-tone backdrop + avatar, positioned at the React cutout rect. */}
      <UiEntity
        uiTransform={{ positionType: 'absolute', position: { left: r.x, top: r.y }, width: r.width, height: r.height }}
        uiBackground={{ color: BACKDROP_BOTTOM }}
      >
        {/* Lighter upper band → a soft two-tone gradient behind the avatar. */}
        <UiEntity uiTransform={{ positionType: 'absolute', position: { top: 0 }, width: '100%', height: '55%' }} uiBackground={{ color: BACKDROP_TOP }} />
        {/* Avatar — camera resolution matches the rect aspect, so a 1:1 fill never distorts.
            Drag over it to rotate (the engine drag-lock passes through React's transparent cutout). */}
        <UiEntity
          uiTransform={{ positionType: 'absolute', width: '100%', height: '100%' }}
          uiBackground={{ videoTexture: { videoPlayerEntity: cameraEntity }, textureMode: 'stretch' }}
          onMouseDragLocked={rotateAvatar}
        />
        {/* "Drag avatar to rotate" hint. */}
        <UiEntity
          uiTransform={{ positionType: 'absolute', position: { bottom: '4%' }, width: '100%', height: 24, justifyContent: 'center', alignItems: 'center' }}
          uiText={{ value: '↻  Drag avatar to rotate', fontSize: 14, color: Color4.create(1, 1, 1, 0.85) }}
        />
      </UiEntity>
    </UiEntity>
  )
}
