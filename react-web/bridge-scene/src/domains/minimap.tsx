// Minimap "Camera" style: a live top-down render of the world, shown through the transparent
// cutout the React minimap carves out (EngineViewport region='map').
//
// The DOM styles (parcel / satellite) are drawn entirely in React from map tiles and never
// reach this file — React tells us which style is active via `minimapConfig`, and we only hold
// a camera while it's 'imposters'. That matters: a TextureCamera is a second render pass, so
// leaving one alive for a map nobody is looking at costs a frame budget for nothing.
//
// The circular mask is drawn HERE rather than in React. React is transparent around the map
// (the world shows through), so if the engine drew a square the corners would spill over the
// world outside the circle — there is no React pixel to cover them.
import ReactEcs, { UiEntity } from '@dcl/react-ecs'
import { CameraLayer, TextureCamera, Transform, engine } from '@dcl/sdk/ecs'
import { Color4, Quaternion, Vector3 } from '@dcl/sdk/math'
import { getPlayer } from '@dcl/sdk/players'
import type { Entity } from '@dcl/ecs'
import type { Ctx } from '../bridge'
import type { MinimapRotation, MinimapStyle } from '../../../src/engine/protocol'

type Rect = { x: number; y: number; width: number; height: number }

// Layer 0 is the real world, so this sees what the player would see looking down — including
// the engine's baked scene imposters at distance, which is where the style's internal name
// ('imposters') comes from.
const LAYER = 0
const ALTITUDE = 201
// Not a flat 90°: pointing exactly down leaves the camera's up vector degenerate.
const PITCH = 89.9
// The render target is sized from the hole the page actually carved out, in PHYSICAL pixels
// (CSS px × dpr), instead of a fixed square: a 220 px minimap on a non-retina display was
// rendering 512² for 220² of visible pixels, i.e. ~5× the fill for no visible gain. Capped so a
// very high dpr or a future bigger minimap can't ask for an unreasonable target, and floored so
// it never degrades into mush.
const MIN_RESOLUTION = 128
const MAX_RESOLUTION = 512
// Below these the camera would move less than the map can show, and writing the Transform is
// not free: it dirties the component, so every frame would serialise a CRDT update to the
// engine. Same thresholds the pose stream uses in `world.ts`.
const MOVE_EPSILON_M = 0.05
const MOVE_EPSILON_DEG = 0.5

let rect: Rect | null = null
let dpr = 1
let cameraEntity: Entity | null = null
let style: MinimapStyle = 'satellite'
let rotation: MinimapRotation = 'north'
let visibleMeters = 256
let appliedMeters = 0
let appliedResolution = 0

/** The square render target for the current rect: physical pixels, clamped. The map is a
 *  circle inside a square hole, so the larger side is what has to be covered. */
function targetResolution(): number {
  if (rect == null) return MIN_RESOLUTION
  const px = Math.max(rect.width, rect.height) * dpr
  return Math.round(Math.min(MAX_RESOLUTION, Math.max(MIN_RESOLUTION, px)))
}
// Last values written to the camera Transform. NaN until the first sync after a (re)create, so
// the comparison below always fails once and the camera is placed before it is ever shown.
let lastX = NaN
let lastZ = NaN
let lastYaw = NaN

/** The Camera style needs both a place to draw and to actually be the selected style. */
function shouldRender(): boolean {
  return rect != null && style === 'imposters'
}

function createCamera(): void {
  if (cameraEntity != null) return
  const c = engine.addEntity()
  CameraLayer.create(c, {
    layer: LAYER,
    directionalLight: false,
    showAvatars: false,
    showSkybox: false,
    showFog: false,
    // Flat, evenly-lit read of the ground; the map is for orientation, not atmosphere.
    ambientBrightnessOverride: 5,
    ambientColorOverride: Color4.White()
  })
  const resolution = targetResolution()
  TextureCamera.create(c, {
    width: resolution,
    height: resolution,
    layer: LAYER,
    clearColor: Color4.create(0, 0, 0, 1),
    mode: { $case: 'orthographic', orthographic: { verticalRange: visibleMeters } },
    // Silent: `volume` attaches an audio receiver to the camera, and this one sits 200 m
    // above the player — anything it picked up would be heard from the wrong place.
    volume: 0
  })
  Transform.create(c, {
    position: Vector3.create(0, ALTITUDE, 0),
    rotation: Quaternion.fromEulerDegrees(PITCH, 0, 0)
  })
  cameraEntity = c
  appliedMeters = visibleMeters
  appliedResolution = resolution
  lastX = NaN
  lastZ = NaN
  lastYaw = NaN
}

function disposeCamera(): void {
  if (cameraEntity == null) return
  engine.removeEntity(cameraEntity)
  cameraEntity = null
}

/** Keep the camera over the player, turned the way the map is turned, at the current zoom. */
function sync(): void {
  if (cameraEntity == null) return
  const pos = getPlayer()?.position
  if (pos == null) return
  const camT = Transform.getOrNull(engine.CameraEntity)
  const camYaw = camT == null ? 0 : Quaternion.toEulerAngles(camT.rotation).y
  // In `north` the map never turns, so a camera spin is not a reason to touch the Transform.
  const yaw = rotation === 'north' ? 0 : camYaw
  // Written as "still" rather than "moved" for the NaN seeding to work: every comparison against
  // NaN is false, so `still` is false on the first sync after a (re)create and the camera gets
  // placed. The inverse (an OR of `>=`) would collapse to false and never move it at all.
  const still =
    Math.abs(pos.x - lastX) < MOVE_EPSILON_M &&
    Math.abs(pos.z - lastZ) < MOVE_EPSILON_M &&
    Math.abs(yaw - lastYaw) < MOVE_EPSILON_DEG
  if (!still) {
    const t = Transform.getMutableOrNull(cameraEntity)
    if (t != null) {
      t.position = Vector3.create(pos.x, ALTITUDE, pos.z)
      t.rotation = Quaternion.fromEulerDegrees(PITCH, yaw, 0)
      lastX = pos.x
      lastZ = pos.z
      lastYaw = yaw
    }
  }
  // Only on change: rebuilding the projection or the render target every frame would be pure
  // churn. The resolution follows the hole's size and the display density, both of which can
  // change under a live camera (collapse/expand keeps the entity, dragging to another monitor
  // changes dpr), so it is re-applied here rather than only at creation.
  const resolution = targetResolution()
  if (visibleMeters !== appliedMeters || resolution !== appliedResolution) {
    const cam = TextureCamera.getMutableOrNull(cameraEntity)
    if (cam != null) {
      cam.mode = { $case: 'orthographic', orthographic: { verticalRange: visibleMeters } }
      cam.width = resolution
      cam.height = resolution
      appliedMeters = visibleMeters
      appliedResolution = resolution
    }
  }
}

export function registerMinimap(ctx: Ctx): void {
  ctx.on('engineViewport', (msg) => {
    if (msg.region !== 'map') return
    rect = msg.rect
    // Older pages don't send it; 1 is the safe read (CSS px == physical px).
    dpr = msg.dpr ?? 1
    if (shouldRender()) createCamera()
    else disposeCamera()
  })

  ctx.on('minimapConfig', (msg) => {
    style = msg.style
    rotation = msg.rotation
    visibleMeters = msg.visibleMeters
    if (shouldRender()) createCamera()
    else disposeCamera()
  })

  ctx.push(() => {
    sync()
  })
}

export function renderMinimap(): ReactEcs.JSX.Element | null {
  if (rect == null || cameraEntity == null) return null
  const r = rect
  return (
    // The rounding has to sit on the entity that carries the background, not on the wrapper —
    // a parent's borderRadius does not clip a child's texture, it only rounds its own fill.
    // The wrapper contributes `overflow: hidden`. Same split the SDK7 HUD used.
    <UiEntity
      uiTransform={{
        positionType: 'absolute',
        position: { left: r.x, top: r.y },
        width: r.width,
        height: r.height,
        overflow: 'hidden'
      }}
    >
      <UiEntity
        uiTransform={{
          positionType: 'absolute',
          position: { top: 0, left: 0 },
          width: r.width,
          height: r.height,
          borderRadius: 9999
        }}
        uiBackground={{ videoTexture: { videoPlayerEntity: cameraEntity }, textureMode: 'stretch' }}
      />
    </UiEntity>
  )
}
