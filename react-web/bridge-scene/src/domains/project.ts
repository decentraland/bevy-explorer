// Shared world→screen projection for the screen-space overlays that pin to 3D points (proximity
// tooltips, avatar nametags). The DOM can't follow a moving 3D position, so the scene projects it
// to screen pixels each frame and relays the coords. Ported from the SDK7 bevy-ui-scene
// `service/perspective-to-screen`; FOV comes from `~system/Runtime.getCameraFov`.
import type { Vec3 } from '../bevy-api'

export type Quat = { x: number; y: number; z: number; w: number }

// v' = v + 2*w*(q×v) + 2*(q×(q×v)) — rotate a vector by a quaternion.
export function rotateByQuat(v: Vec3, q: Quat): Vec3 {
  const cx = q.y * v.z - q.z * v.y
  const cy = q.z * v.x - q.x * v.z
  const cz = q.x * v.y - q.y * v.x
  const ux = q.w * cx + (q.y * cz - q.z * cy)
  const uy = q.w * cy + (q.z * cx - q.x * cz)
  const uz = q.w * cz + (q.x * cy - q.y * cx)
  return { x: v.x + 2 * ux, y: v.y + 2 * uy, z: v.z + 2 * uz }
}

// World point → on-screen pixel coords, or null if behind the camera / off-viewport.
export function projectToScreen(world: Vec3, camPos: Vec3, camRot: Quat, fovY: number, w: number, h: number): { x: number; y: number } | null {
  const inv: Quat = { x: -camRot.x, y: -camRot.y, z: -camRot.z, w: camRot.w }
  const cam = rotateByQuat({ x: world.x - camPos.x, y: world.y - camPos.y, z: world.z - camPos.z }, inv)
  const depth = cam.z // DCL/Unity look down +Z in camera space
  if (depth <= 1e-4) return null // behind camera
  const tanHalf = Math.tan(fovY / 2)
  const ndcX = cam.x / (depth * tanHalf * (w / h))
  const ndcY = cam.y / (depth * tanHalf)
  const x = (ndcX + 1) * 0.5 * w
  const y = (1 - (ndcY + 1) * 0.5) * h
  if (x < 0 || x > w || y < 0 || y > h) return null // off-screen
  return { x, y }
}

const DEFAULT_FOV_Y = 1.0472 // ~60°, until the engine reports the real one

// Vertical FOV tracker — the engine pushes it via Runtime; refresh slowly so projection stays
// accurate without a per-frame async call. `tick(dt)` from a per-frame system.
export function createFovTracker(): { fovY: () => number; tick: (dt: number) => void } {
  let fovY = DEFAULT_FOV_Y
  const runtime = (globalThis as { require?: (m: string) => unknown }).require?.('~system/Runtime') as
    | { getCameraFov?: () => Promise<number> }
    | undefined
  const refresh = (): void => {
    runtime?.getCameraFov?.().then((f) => {
      if (Number.isFinite(f) && f > 0) fovY = f
    }).catch(() => undefined)
  }
  refresh()
  let timer = 0
  return {
    fovY: () => fovY,
    tick: (dt: number) => {
      timer += dt
      if (timer >= 1.5) {
        timer = 0
        refresh()
      }
    }
  }
}
