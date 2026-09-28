// Avatar preview: the live 3D avatar shown in the React Backpack's left column.
//
// React carves a transparent hole (EngineViewport region='avatarPreview') and sends its
// screen rect over the bridge. We render the player's avatar with a dedicated TextureCamera
// (its own CameraLayer, so no world/skybox/other avatars) into a UI videoTexture positioned
// at that rect — the engine composites it behind the React DOM, showing through the hole.
//
// The preview wears the Backpack's look (./avatarDraft), which equipping edits, so equipping in the
// Backpack reflects here before anything is deployed. Mounted via ReactEcsRenderer in index.ts.
import ReactEcs, { UiEntity } from '@dcl/react-ecs'
import { AvatarShape, CameraLayer, CameraLayers, Material, MeshRenderer, PrimaryPointerInfo, TextureCamera, Transform, engine } from '@dcl/sdk/ecs'
import { Color4, Quaternion, Vector3 } from '@dcl/sdk/math'
import { getPlayer } from '@dcl/sdk/players'
import type { Entity } from '@dcl/ecs'
import type { Ctx } from '../bridge'
import type { PreviewFocus } from '../../../src/engine/protocol'
import { currentLook } from './avatarDraft'
import { BevyApi } from '../bevy-api'

type Rect = { x: number; y: number; width: number; height: number }

const LAYER = 10
// The Backpack background behind the avatar: the page's left-column purple, top to bottom.
const BACKDROP_TOP = Color4.create(0.31, 0, 0.565, 1)
const BACKDROP_BOTTOM = Color4.create(0.345, 0.078, 0.514, 1)
const BACKDROP_BANDS = Array.from({ length: 24 }, (_, i) => Color4.lerp(BACKDROP_TOP, BACKDROP_BOTTOM, i / 23))
// Podium layers (gold top, orange ring, dark base). The preview's ambient light is 5×, so these
// are about a fifth of the colours they render as.
const PODIUM_LAYERS = [
  { color: Color4.create(0.34, 0.2, 0.02, 1), scale: 0.96, y: -0.06, height: 0.12 },
  { color: Color4.create(0.4, 0.12, 0.02, 1), scale: 1.01, y: -0.1, height: 0.1 },
  { color: Color4.create(0.07, 0.05, 0.09, 1), scale: 1.05, y: -0.16, height: 0.12 }
]
// Camera framing per focus for the 960×960 preview frame (avatar at 2× scale): the visible height
// (orthographic) and the height it's centred on, measured against the reference: whole body with the
// feet at 91% of the frame, head zoomed 2.5× with the eyes at 38%.
const FRAMING: Record<PreviewFocus, { range: number; centerY: number }> = {
  body: { range: 6.25, centerY: 2.54 },
  head: { range: 2.5, centerY: 3.1 },
  top: { range: 3.96, centerY: 2.8 },
  bottom: { range: 3.96, centerY: 1.15 },
  shoes: { range: 3.03, centerY: 0.35 }
}
const FOCUS_SECONDS = 0.6
const CAMERA_PITCH = 12
const CAMERA_DISTANCE = 8
// The camera is pitched down, so its view centre sits this far below it at the avatar.
const PITCH_DROP = CAMERA_DISTANCE * Math.tan((CAMERA_PITCH * Math.PI) / 180)
// Drag-to-rotate sensitivity (from the SDK7 AvatarPreviewElement).
const ROTATION_FACTOR = -0.5
const FACING = Quaternion.fromEulerDegrees(0, 180, 0)
const THUMBNAIL_SIZE = 480

let rect: Rect | null = null
let avatarEntity: Entity | null = null
let cameraEntity: Entity | null = null
let podiumEntities: Entity[] = []
let lastShapeKey = ''
let framing = FRAMING.body
let framingFrom = FRAMING.body
let framingTo = FRAMING.body
let framingT = 1
// Non-persisting preview override (selecting an item in the Backpack): when set, the preview
// avatar wears these urns instead of the player's actual equipped set. null = no override.
let previewUrns: string[] | null = null
let frameWaiters: Array<{ frames: number; done: () => void }> = []

// Render-target bounds, physical px. Fit inside the engine's 2048 cap here — a one-axis
// engine-side clamp would break the aspect.
const MIN_RES = 256
const MAX_RES = 2048

// The page reports it with each rect; 1 is the safe read (CSS px == physical px).
let dpr = 1

// TextureCamera resolution: the cutout's physical pixels (CSS px x dpr) at its exact aspect,
// so a 1:1 stretch never distorts and the render target follows the window.
function camRes(r: Rect): { width: number; height: number } {
  const aspect = r.height > 0 ? r.width / r.height : 0.55
  let h = Math.min(MAX_RES, Math.max(MIN_RES, Math.round(r.height * dpr)))
  let w = Math.max(64, Math.round(h * aspect))
  if (w > MAX_RES) {
    // ultra-wide cutout: fit the other way, keep the aspect
    w = MAX_RES
    h = Math.max(MIN_RES, Math.round(w / aspect))
  }
  return { width: w, height: h }
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
  const look = currentLook()
  // Preview override (item selected, not equipped) takes precedence over the look's set,
  // so selecting shows the item without adding it to the look.
  const wearables = previewUrns ?? look?.wearables ?? []
  return {
    id: getPlayer()?.userId ?? '',
    bodyShape: look?.bodyShape || undefined,
    eyeColor: look?.eyes ?? undefined,
    hairColor: look?.hair ?? undefined,
    skinColor: look?.skin ?? undefined,
    wearables,
    emotes: [],
    forceRender: look?.forceRender ?? []
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
    rotation: FACING,
    scale: Vector3.create(2, 2, 2)
  })

  // Podium under the avatar (preview layer only), like the platform in the reference backpack.
  const podium = PODIUM_LAYERS.map((layer) => {
    const e = engine.addEntity()
    MeshRenderer.setCylinder(e, 1, 1)
    Material.setPbrMaterial(e, { albedoColor: layer.color, metallic: 0, roughness: 0.6 })
    CameraLayers.create(e, { layers: [LAYER] })
    Transform.create(e, { position: Vector3.create(8, layer.y, 8), scale: Vector3.create(layer.scale, layer.height, layer.scale) })
    return e
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
    mode: { $case: 'orthographic', orthographic: { verticalRange: framing.range } },
    volume: 1
  })
  Transform.create(c, {
    position: Vector3.create(8, framing.centerY + PITCH_DROP, 8 - CAMERA_DISTANCE),
    rotation: Quaternion.fromEulerDegrees(CAMERA_PITCH, 0, 0)
  })

  avatarEntity = a
  cameraEntity = c
  podiumEntities = podium
  lastShapeKey = ''
  syncShape()
}

function disposePreview(): void {
  if (avatarEntity != null) engine.removeEntity(avatarEntity)
  if (cameraEntity != null) engine.removeEntity(cameraEntity)
  podiumEntities.forEach((e) => engine.removeEntity(e))
  avatarEntity = null
  cameraEntity = null
  podiumEntities = []
}

// Re-read the player's avatar into the shape when it changes (equip via React → setAvatar).
function syncShape(): void {
  if (avatarEntity == null) return
  const shape = avatarShape()
  const key = JSON.stringify(shape)
  if (key === lastShapeKey) return
  lastShapeKey = key
  const mut = AvatarShape.getMutableOrNull(avatarEntity)
  if (mut != null) Object.assign(mut, shape)
}

export function registerAvatarPreview(ctx: Ctx): void {
  ctx.on('engineViewport', (msg) => {
    if (msg.region !== 'avatarPreview') return
    rect = msg.rect
    dpr = msg.dpr ?? 1
    if (rect == null) {
      disposePreview()
      return
    }
    if (avatarEntity == null) createPreview()
    else if (cameraEntity != null) {
      // Window resized → re-size the render target to the hole, not just re-aspect it.
      const res = camRes(rect)
      const cam = TextureCamera.getMutableOrNull(cameraEntity)
      if (cam != null && (cam.width !== res.width || cam.height !== res.height)) {
        cam.width = res.width
        cam.height = res.height
      }
    }
  })

  // The Backpack's selected category: ease the camera to frame that part of the avatar.
  ctx.on('previewFocus', (msg) => {
    const to = FRAMING[msg.focus] ?? FRAMING.body
    if (to === framingTo) return
    framingFrom = framing
    framingTo = to
    framingT = 0
  })
  ctx.push(() => {
    const due = frameWaiters.filter((w) => --w.frames <= 0)
    frameWaiters = frameWaiters.filter((w) => w.frames > 0)
    due.forEach((w) => {
      w.done()
    })
  })
  ctx.push((dt) => {
    if (framingT >= 1 || cameraEntity == null) return
    framingT = Math.min(1, framingT + dt / FOCUS_SECONDS)
    const e = framingT * framingT * (3 - 2 * framingT)
    framing = {
      range: framingFrom.range + (framingTo.range - framingFrom.range) * e,
      centerY: framingFrom.centerY + (framingTo.centerY - framingFrom.centerY) * e
    }
    applyFraming(framing)
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

export async function waitFrames(frames: number): Promise<void> {
  await new Promise<void>((resolve) => {
    frameWaiters.push({ frames, done: resolve })
  })
}

function applyFraming(f: { range: number; centerY: number }): void {
  if (cameraEntity == null) return
  const cam = TextureCamera.getMutableOrNull(cameraEntity)
  if (cam?.mode?.$case === 'orthographic') cam.mode.orthographic.verticalRange = f.range
  Transform.getMutable(cameraEntity).position = Vector3.create(8, f.centerY + PITCH_DROP, 8 - CAMERA_DISTANCE)
}

/** The preview avatar, whole body, facing forward, without the podium, as a PNG data URL — or null
 *  when the preview isn't open or the engine can't capture. */
export async function captureAvatarThumbnail(): Promise<string | null> {
  if (avatarEntity == null || cameraEntity == null || BevyApi.consoleCommand == null) return null
  const avatar = avatarEntity
  const rotation = Transform.get(avatar).rotation
  Transform.getMutable(avatar).rotation = FACING
  framingT = 1
  applyFraming(FRAMING.body)
  const podium = podiumEntities
  podium.forEach((e) => { Transform.getMutable(e).scale = Vector3.Zero() })
  try {
    await waitFrames(3)
    const png = await BevyApi.consoleCommand('texture_camera_screenshot', [String(LAYER), String(THUMBNAIL_SIZE)])
    return `data:image/png;base64,${png}`
  } catch (e) {
    console.error('[avatarPreview] thumbnail capture failed', e)
    return null
  } finally {
    if (avatarEntity === avatar) {
      Transform.getMutable(avatar).rotation = rotation
      if (framingT >= 1) framing = framingTo
      applyFraming(framing)
      podium.forEach((e, i) => {
        const layer = PODIUM_LAYERS[i]
        Transform.getMutable(e).scale = Vector3.create(layer.scale, layer.height, layer.scale)
      })
    }
  }
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
        {/* The page's left-column purple, as thin bands so it reads as a smooth gradient. */}
        {BACKDROP_BANDS.map((color, i) => (
          <UiEntity
            key={i}
            uiTransform={{ positionType: 'absolute', position: { top: `${(i * 100) / BACKDROP_BANDS.length}%` }, width: '100%', height: `${100 / BACKDROP_BANDS.length + 0.5}%` }}
            uiBackground={{ color }}
          />
        ))}
        {/* Avatar — camera resolution matches the rect aspect, so a 1:1 fill never distorts.
            Drag over it to rotate (the engine drag-lock passes through React's transparent cutout). */}
        <UiEntity
          uiTransform={{ positionType: 'absolute', width: '100%', height: '100%' }}
          uiBackground={{ videoTexture: { videoPlayerEntity: cameraEntity }, textureMode: 'stretch' }}
          onMouseDragLocked={rotateAvatar}
        />
      </UiEntity>
    </UiEntity>
  )
}
