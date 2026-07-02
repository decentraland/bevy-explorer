// Settings: the explorer settings list + changing one.
//   from: BevyApi.getSettings() / setSetting().
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

export function registerSettings(ctx: Ctx): void {
  const pushSettings = (): void => {
    BevyApi.getSettings()
      .then((settings) => {
        ctx.send({ kind: 'settings', settings })
      })
      .catch((e: unknown) => {
        console.error('[settings] getSettings failed', e)
      })
  }

  ctx.on('getSettings', () => {
    pushSettings()
  })
  ctx.on('setSetting', (msg) => {
    BevyApi.setSetting(msg.name, msg.value)
      .then(pushSettings)
      .catch((e: unknown) => {
        console.error('[settings] setSetting failed', e)
      })
  })

  pushSettings() // initial snapshot
}
