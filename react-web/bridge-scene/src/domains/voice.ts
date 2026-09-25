// Voice activity: who is talking right now.
//   from: BevyApi.getVoiceStream() (LiveKit active-speaker changes, local player included)
//   to:   the page (Nearby list) + the nametag speaking badge (isSpeaking)
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

const speaking = new Set<string>()

export function isSpeaking(address: string): boolean {
  return speaking.has(address.toLowerCase())
}

export function registerVoice(ctx: Ctx): void {
  void (async () => {
    try {
      const stream = await BevyApi.getVoiceStream()
      for await (const v of stream) {
        const address = v.sender_address.toLowerCase()
        if (v.active) speaking.add(address)
        else speaking.delete(address)
        ctx.send({ kind: 'voiceActivity', address, active: v.active })
      }
    } catch (e) {
      console.error('[voice] stream relay failed', e)
    }
  })()
}
