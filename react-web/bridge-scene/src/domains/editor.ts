// Editor: spawns and kills the scene editor's own super-user scene for the page's editor host.
// A scene spawned at runtime is not a startup scene, so the engine checks its permissions like
// any portable's: the ones it needs are granted to that one scene hash, before it runs.
//   from: BevyApi.consoleCommand (spawn / kill), setPermanentPermission, liveSceneInfo.
import { BevyApi } from '../bevy-api'
import { waitMs } from '../system-helpers'
import type { Ctx } from '../bridge'

// The editor's free camera, pinning the avatar while it flies, and moving the avatar.
const PERMISSIONS = ['ForceCamera', 'SetLocomotion', 'MovePlayer']
const LIVE_TIMEOUT_MS = 30_000
const LIVE_POLL_MS = 500

function setPermissions(hash: string, allow: 'Allow' | null): void {
  for (const ty of PERMISSIONS) BevyApi.setPermanentPermission({ level: 'Scene', value: hash, ty, allow })
}

async function waitLive(hash: string): Promise<void> {
  for (let waited = 0; waited < LIVE_TIMEOUT_MS; waited += LIVE_POLL_MS) {
    const scene = (await BevyApi.liveSceneInfo()).find((s) => s.hash === hash)
    if (scene != null) {
      if (!scene.isSuper || scene.isBroken) throw new Error('the editor scene started without its privileges')
      return
    }
    await waitMs(LIVE_POLL_MS)
  }
  throw new Error('the editor scene did not start')
}

export function registerEditor(ctx: Ctx): void {
  ctx.on('editorScene', async (msg) => {
    try {
      const run = BevyApi.consoleCommand
      if (run == null) throw new Error('the engine console is not available')
      if (msg.action === 'spawn') {
        setPermissions(msg.hash, 'Allow')
        try {
          await run('spawn', [msg.source, 'true'])
          await waitLive(msg.hash)
        } catch (e) {
          setPermissions(msg.hash, null)
          throw e
        }
      } else {
        try {
          await run('kill', [msg.source])
        } finally {
          setPermissions(msg.hash, null)
        }
      }
      ctx.send({ kind: 'editorSceneResult', id: msg.id, ok: true })
    } catch (e) {
      ctx.send({ kind: 'editorSceneResult', id: msg.id, ok: false, error: e instanceof Error ? e.message : String(e) })
    }
  })
}
