// Direct messages: per-partner state streams and local history.
//   from: BevyApi.social.getDmUserStateStream / getDmHistory / deleteDmHistory
// DMs themselves ride the chat stream (chat.ts) with the partner wallet as the channel.
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'
import type { DmUserState } from '../../../src/engine/protocol'

const STATES = new Set<DmUserState>([
  'notConnected',
  'connected',
  'blockedByOwnUser',
  'privateMessagesBlockedByOwnUser',
  'privateMessagesBlocked',
  'disconnected',
  'otherClient'
])

export function registerDm(ctx: Ctx): void {
  const social = BevyApi.social
  // One engine stream per watched partner, for as long as the page shows that conversation.
  const watches = new Map<string, { close: () => void }>()

  ctx.on('dmWatch', (msg) => {
    const key = msg.address.toLowerCase()
    const current = watches.get(key)
    if (!msg.on) {
      current?.close()
      watches.delete(key)
      return
    }
    if (current != null) return
    const stream = social.getDmUserStateStream(msg.address)
    watches.set(key, stream)
    void (async () => {
      try {
        for await (const s of stream) {
          const state = STATES.has(s.state as DmUserState) ? (s.state as DmUserState) : 'disconnected'
          ctx.send({ kind: 'dmUserState', address: s.address, state, online: s.online })
        }
      } catch (e) {
        console.error('[dm] state stream failed', e)
      } finally {
        if (watches.get(key) === stream) watches.delete(key)
      }
    })()
  })

  ctx.on('dmHistory', async (msg) => {
    const entries = await social.getDmHistory(msg.address).catch((e) => {
      console.error('[dm] history failed', e)
      return []
    })
    ctx.send({
      kind: 'dmHistory',
      address: msg.address,
      entries: entries.map((e) => ({ from: e.from, message: e.message, receivedAt: e.receivedAt * 1000 }))
    })
  })

  ctx.on('dmDelete', (msg) => {
    social.deleteDmHistory(msg.address)
  })
}
