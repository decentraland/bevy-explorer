// Input bindings: the engine's binding table + rebinding.
//   from: BevyApi.getInputBindings() / setInputBindings() / getNativeInput().
//
// The table is pushed once at registration (shortcut hints render without a round-trip) and
// re-fetched after every set/reset so React always observes the engine's actual state rather
// than assuming its request applied. Capture ('press a key') uses getNativeInput, which has
// no engine-side cancel — a dismissed capture resolves on the NEXT input pressed — so every
// capture carries the page's request id and only the newest pending id is relayed; the page
// additionally drops responses for ids it no longer waits on.
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

export function registerBindings(ctx: Ctx): void {
  const pushBindings = async (): Promise<void> => {
    const { bindings } = await BevyApi.getInputBindings()
    ctx.send({ kind: 'bindings', bindings })
  }

  ctx.on('getBindings', pushBindings)

  ctx.on('setBindings', async (msg) => {
    await BevyApi.setInputBindings({ bindings: msg.bindings })
    await pushBindings()
  })

  ctx.on('resetBindings', async () => {
    if (BevyApi.consoleCommand == null) return
    await BevyApi.consoleCommand('reset_controls', [])
    await pushBindings()
  })

  // HUD focus relay: fire-and-forget, latest state wins (the engine reserves/releases
  // idempotently, so replays are harmless).
  ctx.on('uiFocus', (msg) => {
    void BevyApi.setUiFocus({ ui: msg.ui, text: msg.text, scroll: msg.scroll })
  })

  let activeCaptureId: string | null = null
  ctx.on('captureInput', (msg) => {
    activeCaptureId = msg.id
    void BevyApi.getNativeInput().then((input) => {
      if (activeCaptureId !== msg.id) return // superseded — a stale resolve, drop it
      activeCaptureId = null
      ctx.send({ kind: 'inputCaptured', id: msg.id, input })
    })
  })

  void pushBindings()
}
