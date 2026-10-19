// Permissions: relays a scene's permission prompts (e.g. ChangeRealm) to the React HUD and
// sends the user's decision back; relays each permission a scene uses (allowed or refused) so the
// HUD can list them, and rewrites a stored rule when the player revisits one from that list. The
// engine only runs this RPC path when native UI permissions are off (the react-web build) —
// otherwise it draws its own Bevy dialog.
//   from: SystemApi.getPermissionRequestStream / getPermissionUsedStream / setSinglePermission /
//   setPermanentPermission / getPermanentPermissions / consoleCommand('reset_permissions').
import { getRealm } from '~system/Runtime'
import { BevyApi } from '../bevy-api'
import { relay } from '../system-helpers'
import type { Ctx } from '../bridge'

// A scene exercising a permission every frame (Fetch, MovePlayer) reports a use every frame; the
// HUD keeps one entry per scene + type, so repeated allows within this window are not sent. Every
// refusal is: the HUD skips the one the player clicked and counts the rest.
const USED_REPEAT_MS = 2000

export function registerPermissions(ctx: Ctx): void {
  // scene hash → title, filled from liveSceneInfo on first sight so a busy scene costs one lookup
  const titles = new Map<string, string>()
  const sceneTitle = async (hash: string): Promise<string> => {
    const known = titles.get(hash)
    if (known != null) return known
    let title = 'A scene'
    try {
      title = (await BevyApi.liveSceneInfo()).find((s) => s.hash === hash)?.title ?? title
    } catch (e) {
      console.error('[permissions] liveSceneInfo failed', e)
    }
    titles.set(hash, title)
    return title
  }

  relay('permissions', async () => await BevyApi.getPermissionRequestStream(), async (req) => {
    if (req.type === 'cancelled') {
      // resolved engine-side (an "Always …" answer to an earlier prompt covers it): drop the prompt
      ctx.send({ kind: 'permissionWithdrawn', id: req.id })
      return
    }
    // The engine's request only carries the scene HASH. Resolve the title (hash→title) and the
    // current realm from the existing SystemApi — same as the SDK7 scene, so no Rust change.
    // Both are best-effort: a failure must never drop the request (else no dialog ever shows).
    let sceneName = 'A scene'
    let realm = ''
    try {
      sceneName = (await BevyApi.liveSceneInfo()).find((s) => s.hash === req.scene)?.title ?? 'A scene'
    } catch (e) {
      console.error('[permissions] liveSceneInfo failed', e)
    }
    try {
      realm = await BevyApi.getRealmProvider()
    } catch (e) {
      console.error('[permissions] getRealmProvider failed', e)
    }
    ctx.send({
      kind: 'permissionRequest',
      id: req.id,
      ty: req.ty,
      sceneName,
      scene: req.scene,
      realm,
      additional: req.additional ?? undefined
    })
  })

  // The realm a rule is stored under: the engine writes every scene's RealmInfo (the bridge's
  // included) from the realm it is connected to, as the about url without its `/about`. The
  // engine matches that as a prefix of the about url it keys realm rules by.
  const realmProvider = async (): Promise<string> => {
    try {
      return (await getRealm({})).realmInfo?.baseUrl ?? ''
    } catch (e) {
      console.error('[permissions] getRealm failed', e)
      return ''
    }
  }

  const lastSent = new Map<string, number>()
  relay('permissionsUsed', async () => await BevyApi.getPermissionUsedStream(), async (use) => {
    if (use.wasAllowed) {
      const key = `${use.scene}|${use.ty}`
      const now = Date.now()
      const last = lastSent.get(key)
      if (last != null && now - last < USED_REPEAT_MS) return
      lastSent.set(key, now)
    }
    ctx.send({
      kind: 'permissionUsed',
      ty: use.ty,
      sceneName: await sceneTitle(use.scene),
      scene: use.scene,
      realm: await realmProvider(),
      allowed: use.wasAllowed
    })
  })

  // The review dialog only rewrites stored rules — there is no request to resolve. 'visit'
  // forgets the type at every level (the page answers from then on), so an accidental "Always deny
  // for Global" is one click to undo. Below Global sits only the built-in default, Allow for some
  // types, so a cleared Global rule is stored as Ask: the scene asks, rather than silently acting.
  ctx.on('getPermissionRules', async (msg) => {
    const realm = await realmProvider()
    const at = async (level: 'Scene' | 'Realm' | 'Global', value?: string): Promise<Array<{ ty: string; allow: 'Allow' | 'Deny' | 'Ask' }>> =>
      (await BevyApi.getPermanentPermissions({ level, value }).catch((e) => {
        console.error('[permissions] getPermanentPermissions failed', e)
        return []
      })).map((item) => ({ ty: item.ty, allow: item.allow }))
    const [scene, realmRules, global] = await Promise.all([at('Scene', msg.scene), at('Realm', realm), at('Global')])
    ctx.send({ kind: 'permissionRules', scene: msg.scene, realm, rules: { scene, realm: realmRules, global } })
  })

  ctx.on('resetPermissions', async () => {
    if (BevyApi.consoleCommand == null) return
    await BevyApi.consoleCommand('reset_permissions', [])
  })

  ctx.on('permissionRule', (msg) => {
    if (msg.level === 'visit') {
      BevyApi.setPermanentPermission({ level: 'Scene', value: msg.scene, ty: msg.ty, allow: null })
      BevyApi.setPermanentPermission({ level: 'Realm', value: msg.realm, ty: msg.ty, allow: null })
      BevyApi.setPermanentPermission({ level: 'Global', ty: msg.ty, allow: 'Ask' })
      return
    }
    const level = msg.level === 'scene' ? 'Scene' : msg.level === 'realm' ? 'Realm' : 'Global'
    const value = msg.level === 'scene' ? msg.scene : msg.level === 'realm' ? msg.realm : undefined
    const cleared = level === 'Global' ? 'Ask' : null
    BevyApi.setPermanentPermission({ level, value, ty: msg.ty, allow: msg.allow == null ? cleared : msg.allow ? 'Allow' : 'Deny' })
  })

  // 'once' and 'visit' resolve just this request (the page keeps the visit memory); the
  // "Always for …" levels persist a permanent rule,
  // which the engine immediately applies to the outstanding request (no separate resolve).
  ctx.on('permissionResolve', (msg) => {
    if (msg.level === 'once' || msg.level === 'visit') {
      BevyApi.setSinglePermission({ id: msg.id, allow: msg.allow })
      return
    }
    const level = msg.level === 'scene' ? 'Scene' : msg.level === 'realm' ? 'Realm' : 'Global'
    const value = msg.level === 'scene' ? msg.scene : msg.level === 'realm' ? msg.realm : undefined
    BevyApi.setPermanentPermission({ level, value, ty: msg.ty, allow: msg.allow ? 'Allow' : 'Deny' })
  })
}
