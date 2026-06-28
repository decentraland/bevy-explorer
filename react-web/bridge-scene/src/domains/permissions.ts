// Permissions: relays a scene's permission prompts (e.g. ChangeRealm) to the React HUD and
// sends the user's decision back. The engine only runs this RPC path when native UI permissions
// are off (the react-web build) — otherwise it draws its own Bevy dialog.
//   from: SystemApi.getPermissionRequestStream / setSinglePermission / setPermanentPermission.
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

export function registerPermissions(ctx: Ctx): void {
  void (async () => {
    try {
      const stream = await BevyApi.getPermissionRequestStream()
      for await (const req of stream) {
        ctx.send({
          kind: 'permissionRequest',
          id: req.id,
          ty: req.ty,
          sceneName: req.scene_name,
          scene: req.scene,
          realm: req.realm,
          additional: req.additional ?? undefined
        })
      }
    } catch (e) {
      console.error('[permissions] stream failed', e)
    }
  })()

  // "Once" resolves just this request; the "Always for …" levels persist a permanent rule,
  // which the engine immediately applies to the outstanding request (no separate resolve).
  ctx.on('permissionResolve', (msg) => {
    if (msg.level === 'once') {
      BevyApi.setSinglePermission({ id: msg.id, allow: msg.allow })
      return
    }
    const level = msg.level === 'scene' ? 'Scene' : msg.level === 'realm' ? 'Realm' : 'Global'
    const value = msg.level === 'scene' ? msg.scene : msg.level === 'realm' ? msg.realm : undefined
    BevyApi.setPermanentPermission({ level, value, ty: msg.ty, allow: msg.allow ? 'Allow' : 'Deny' })
  })
}
