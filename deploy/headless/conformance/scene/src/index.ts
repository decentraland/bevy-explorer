import {
  engine,
  Transform,
  MeshCollider,
  Tween,
  TweenState,
  tweenSystem,
  EasingFunction,
  raycastSystem,
  RaycastQueryType,
  ColliderLayer
} from '@dcl/sdk/ecs'
import { Vector3 } from '@dcl/sdk/math'
import { isServer } from '~system/EngineApi'

// CI conformance fixture for the headless server engine. Each assertion prints
// exactly one `[CONFORMANCE] <name> ...` line; the runner greps for the full set.
// Deliberately avoids @dcl/sdk/network|players|server so it needs no comms.

const TARGET = Vector3.create(8, 1, 8)

export function main() {
  console.log('[CONFORMANCE] boot')

  void isServer({}).then((r) => console.log('[CONFORMANCE] is-server', r.isServer))

  // raycast against a known collider: expect a hit at z=7 (front face)
  const target = engine.addEntity()
  Transform.create(target, { position: TARGET, scale: Vector3.create(2, 2, 2) })
  MeshCollider.setBox(target, ColliderLayer.CL_PHYSICS)
  const origin = engine.addEntity()
  Transform.create(origin, { position: Vector3.create(8, 1, 2) })
  let raycastDone = false
  raycastSystem.registerGlobalTargetRaycast(
    {
      entity: origin,
      opts: {
        queryType: RaycastQueryType.RQT_HIT_FIRST,
        target: TARGET,
        maxDistance: 30,
        collisionMask: ColliderLayer.CL_PHYSICS,
        continuous: true
      }
    },
    (result) => {
      if (raycastDone || result.hits.length === 0) return
      raycastDone = true
      const hit = result.hits[0]
      const p = hit.position
      console.log('[CONFORMANCE] raycast-hit', p ? `${p.x.toFixed(1)},${p.y.toFixed(1)},${p.z.toFixed(1)}` : 'no-position')
      raycastSystem.removeRaycasterEntity(origin)
    }
  )

  // tween: 1s shuttle; expect TweenState to progress and completion to fire
  const shuttle = engine.addEntity()
  Transform.create(shuttle, { position: Vector3.create(2, 1, 2) })
  Tween.create(shuttle, {
    mode: Tween.Mode.Move({ start: Vector3.create(2, 1, 2), end: Vector3.create(14, 1, 2) }),
    duration: 1000,
    easingFunction: EasingFunction.EF_LINEAR
  })
  let sawState = false
  let sawCompleted = false
  engine.addSystem(() => {
    if (!sawState && TweenState.getOrNull(shuttle)) {
      sawState = true
      console.log('[CONFORMANCE] tween-state')
    }
    if (!sawCompleted && tweenSystem.tweenCompleted(shuttle)) {
      sawCompleted = true
      console.log('[CONFORMANCE] tween-completed')
    }
  })
}
