// Chat: incoming messages and reactions, sending both, and the nearby-players roster.
//   from: BevyApi.getChatStream() / sendChat(), getChatReactionStream() / sendChatReaction(),
//         @dcl/sdk PlayerIdentityData (nearby roster)
//         + the ENGINE's profile cache (~system/Players getPlayerData) for faces.
import { engine, PlayerIdentityData, PointerLock } from '@dcl/sdk/ecs'
import { getPlayer } from '@dcl/sdk/players'
import { getPlayerData } from '~system/Players'
import { BevyApi } from '../bevy-api'
import { httpOrUndef, profileKey } from './profile'
import { setChatBubble } from './nametags'
import { onSystemAction } from './systemAction'
import { relay } from '../system-helpers'
import type { Ctx } from '../bridge'
import type { NearbyMember } from '../../../src/engine/protocol'
import { mentionName, mentionsName } from '../../../src/engine/mention'

// The local player's claimed-name flag, from the engine's profile (unknown until it loads).
let selfClaimed: boolean | undefined
// Does a message @-mention the local player (so their bubble border highlights)? Exactly their
// mention name, the same rule the HUD uses: an unclaimed Name is only @Name#1a2b.
function mentionsMe(message: string): boolean {
  const me = getPlayer()
  if (me?.name == null || me.name === '' || me.userId == null) return false
  return mentionsName(message, mentionName(me.name, me.userId, selfClaimed))
}

export function registerChat(ctx: Ctx): void {
  // React → engine.
  ctx.on('sendChat', (msg) => {
    BevyApi.sendChat(msg.message, msg.channel)
  })
  ctx.on('sendChatReaction', (msg) => {
    BevyApi.sendChatReaction(msg.channel, msg.messageId, msg.emoji, msg.remove)
  })

  // Incoming chat stream → React (we're the only consumer now the SDK7 chat UI is gone).
  relay('chat', async () => await BevyApi.getChatStream(), (m) => {
    if (m.message.indexOf('␑') === 0) return // engine control message
    ctx.send({ kind: 'chat', chat: { sender: m.sender_address, message: m.message, channel: m.channel, messageId: m.message_id } })
    // Pop the speech bubble under this sender's nametag (world-space, engine-positioned). Not for
    // DMs (a wallet channel): those are private.
    if (m.channel === 'Nearby') setChatBubble(m.sender_address, m.message, mentionsMe(m.message))
  })

  relay('chatReaction', async () => await BevyApi.getChatReactionStream(), (r) => {
    ctx.send({
      kind: 'chatReaction',
      reaction: { channel: r.channel, messageId: r.message_id, emoji: r.emoji, from: r.from, remove: r.remove }
    })
  })

  // Enter → focus chat, on both native (the engine reads keys off the OS window) and web (winit
  // sees window-level keys): the engine's "Chat" action becomes a dedicated focusChat message.
  // The page deliberately doesn't map 'Chat' in its systemAction dispatcher — this is the one
  // path. systemAction.ts owns the stream (single-consumer per scene); we subscribe here.
  let freeCursorPending = false
  onSystemAction((a) => {
    if (a.action === 'Chat' && a.pressed) {
      freeCursorPending = true
      ctx.send({ kind: 'focusChat' })
    }
  })

  // Release the engine's camera-look on NATIVE when Enter opens chat: writing isPointerLocked=false on
  // CameraEntity frees the engine's OS cursor grab so the mouse stops driving the camera while you type
  // (chat focus is `uiFocus.text`, not `ui`, so the engine doesn't free it itself). On web this write
  // isn't reached — the engine never sees Enter there — so the release is page-side (requestFocusChat
  // calls document.exitPointerLock). Must run in a frame system, NOT the async callback above: an async
  // component write doesn't flush to the engine (the same reason nametag chat bubbles defer their writes).
  ctx.push(() => {
    if (!freeCursorPending) return
    freeCursorPending = false
    const pl = PointerLock.getMutableOrNull(engine.CameraEntity)
    if (pl != null) pl.isPointerLocked = false
  })

  // Nearby players (PlayerIdentityData set) → chat header "Nearby · N". Poll ~3s, push on change.
  // Faces come from the engine's profile cache, which already holds every nearby player's profile
  // for their nametag: one RPC per address, answered once the engine has resolved it. An address
  // it couldn't resolve is dropped, so the next tick asks again.
  const faces = new Map<string, string | undefined>()
  const claimed = new Map<string, boolean>()
  // Re-asked when the player's name changes, so claiming a name mid-session is picked up.
  let selfAskedFor: string | null = null
  let acc = 3
  let lastKey = ''
  ctx.push((dt) => {
    acc += dt
    if (acc < 3) return
    acc = 0
    const me = getPlayer()
    const selfKey = me?.userId != null ? `${me.userId}|${me.name}` : null
    if (selfKey != null && selfKey !== selfAskedFor && me?.userId != null) {
      selfAskedFor = selfKey
      getPlayerData({ userId: me.userId })
        .then((res) => (selfClaimed = (res.data as { hasClaimedName?: boolean } | undefined)?.hasClaimedName === true))
        .catch(() => (selfAskedFor = null))
    }
    const members: NearbyMember[] = []
    for (const [, data] of engine.getEntitiesWith(PlayerIdentityData)) {
      const address = data.address
      const key = profileKey(address)
      if (!faces.has(key)) {
        faces.set(key, undefined)
        getPlayerData({ userId: address })
          .then((res) => {
            faces.set(key, httpOrUndef(res.data?.avatar?.snapshots?.face256))
            claimed.set(key, (res.data as { hasClaimedName?: boolean } | undefined)?.hasClaimedName === true)
          })
          .catch(() => {
            faces.delete(key)
            claimed.delete(key)
          })
      }
      members.push({
        address,
        name: getPlayer({ userId: address })?.name ?? '',
        picture: faces.get(key),
        claimed: claimed.get(key)
      })
    }
    // Forget players who left, so the caches don't grow for the whole session.
    const present = new Set(members.map((m) => profileKey(m.address)))
    for (const k of faces.keys()) if (!present.has(k)) faces.delete(k)
    for (const k of claimed.keys()) if (!present.has(k)) claimed.delete(k)
    const key = members.map((m) => `${m.address}:${m.picture ?? ''}:${String(m.claimed)}`).sort().join(',')
    if (key === lastKey) return
    lastKey = key
    ctx.send({ kind: 'members', members })
  })
}
