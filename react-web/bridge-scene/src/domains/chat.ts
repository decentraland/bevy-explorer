// Chat: incoming messages, sending, and the nearby-players roster.
//   from: BevyApi.getChatStream() / sendChat(), @dcl/sdk PlayerIdentityData (nearby roster)
//         + the ENGINE's profile cache (~system/Players getPlayerData) for faces.
import { engine, PlayerIdentityData, PointerLock } from '@dcl/sdk/ecs'
import { getPlayer } from '@dcl/sdk/players'
import { getPlayerData } from '~system/Players'
import { BevyApi } from '../bevy-api'
import { httpOrUndef, profileKey } from './profile'
import { setChatBubble } from './nametags'
import { onSystemAction } from './systemAction'
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
  // (mirrors the profile-card free-cursor). On web this write isn't reached — the engine never sees
  // Enter there — so the release is page-side (requestFocusChat calls document.exitPointerLock). Must
  // run in a frame system, NOT the async callback above: an async component write doesn't flush to the
  // engine (the same reason nametag chat bubbles defer their writes).
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
  let acc = 3
  let lastKey = ''
  ctx.push((dt) => {
    acc += dt
    if (acc < 3) return
    acc = 0
    const members: NearbyMember[] = []
    for (const [, data] of engine.getEntitiesWith(PlayerIdentityData)) {
      const address = data.address
      const key = profileKey(address)
      if (!faces.has(key)) {
        faces.set(key, undefined)
        getPlayerData({ userId: address })
          .then((res) => faces.set(key, httpOrUndef(res.data?.avatar?.snapshots?.face256)))
          .catch(() => faces.delete(key))
      }
      members.push({
        address,
        name: getPlayer({ userId: address })?.name ?? '',
        picture: faces.get(key)
      })
    }
    const key = members.map((m) => `${m.address}:${m.picture ?? ''}`).sort().join(',')
    if (key === lastKey) return
    lastKey = key
    ctx.send({ kind: 'members', members })
  })
}
