// Chat: incoming messages, sending, and the nearby-players roster.
//   from: BevyApi.getChatStream() / sendChat(), @dcl/sdk PlayerIdentityData (nearby roster)
//         + the catalyst profile cache (profile.ts) for faces.
import { engine, PlayerIdentityData } from '@dcl/sdk/ecs'
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import { fetchProfile, profileCache } from './profile'
import { setChatBubble } from './nametags'
import type { Ctx } from '../bridge'
import type { NearbyMember } from '../../../src/engine/protocol'

// Does a message @-mention the local player (so their bubble border highlights)? Matches `@<name>`
// against the local player's base name (case-insensitive) — the same heuristic the React chat uses.
function mentionsMe(message: string): boolean {
  const me = getPlayer()?.name?.split('#')[0]?.toLowerCase()
  return me != null && me !== '' && message.toLowerCase().includes(`@${me}`)
}

export function registerChat(ctx: Ctx): void {
  // React → engine.
  ctx.on('sendChat', (msg) => {
    BevyApi.sendChat(msg.message, msg.channel)
  })

  // Incoming chat stream → React (we're the only consumer now the SDK7 chat UI is gone).
  void (async () => {
    try {
      const stream = await BevyApi.getChatStream()
      for await (const m of stream) {
        if (m.message.indexOf('␑') === 0) continue // engine control message
        ctx.send({ kind: 'chat', chat: { sender: m.sender_address, message: m.message, channel: m.channel } })
        // Pop the speech bubble under this sender's nametag (world-space, engine-positioned).
        setChatBubble(m.sender_address, m.message, mentionsMe(m.message))
      }
    } catch (e) {
      console.error('[chat] stream relay failed', e)
    }
  })()

  // Nearby players (PlayerIdentityData set) → chat header "Nearby · N". Poll ~3s, push on change.
  let acc = 3
  let lastKey = ''
  ctx.push((dt) => {
    acc += dt
    if (acc < 3) return
    acc = 0
    const members: NearbyMember[] = []
    for (const [, data] of engine.getEntitiesWith(PlayerIdentityData)) {
      const address = data.address
      if (!profileCache.has(address)) {
        void fetchProfile(address).catch(() => undefined)
      }
      const face = profileCache.get(address)?.avatars?.[0]?.avatar?.snapshots?.face256
      members.push({
        address,
        name: getPlayer({ userId: address })?.name ?? '',
        picture: typeof face === 'string' && face.startsWith('http') ? face : undefined
      })
    }
    const key = members.map((m) => `${m.address}:${m.picture ?? ''}`).sort().join(',')
    if (key === lastKey) return
    lastKey = key
    ctx.send({ kind: 'members', members })
  })
}
