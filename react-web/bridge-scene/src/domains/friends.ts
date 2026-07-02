// Friends: list, requests, blocked, and friend actions.
//   from: BevyApi.social.* (the engine's authenticated social-service client).
// No store: we poll the social API directly (~every 1s) and push on change, so the whole
// friends surface is right here in one file.
import { BevyApi, type FriendStatusData, type FriendRequestData } from '../bevy-api'
import type { Ctx } from '../bridge'
import type { Friend, FriendRequest } from '../../../src/engine/protocol'

const toFriend = (f: FriendStatusData): Friend => ({
  address: f.address,
  name: f.name,
  picture: f.profilePictureUrl !== '' ? f.profilePictureUrl : undefined,
  status: f.status
})
const toRequest = (r: FriendRequestData): FriendRequest => ({
  address: r.address,
  name: r.name,
  picture: r.profilePictureUrl !== '' ? r.profilePictureUrl : undefined,
  message: r.message,
  id: r.id,
  createdAt: r.createdAt
})

export function registerFriends(ctx: Ctx): void {
  const social = BevyApi.social

  // Actions React triggers (accept/reject/cancel/delete/block/unblock).
  ctx.on('friendAction', (msg) => {
    const a = msg.address
    const run: Promise<unknown> =
      msg.op === 'request' ? social.sendFriendRequest(a)
        : msg.op === 'accept' ? social.acceptFriendRequest(a)
          : msg.op === 'reject' ? social.rejectFriendRequest(a)
            : msg.op === 'cancel' ? social.cancelFriendRequest(a)
              : msg.op === 'delete' ? social.deleteFriend(a)
                : msg.op === 'block' ? social.blockUser(a)
                  : social.unblockUser(a)
    run.catch((e: unknown) => {
      console.error('[friends] action failed', e)
    })
  })

  // Poll the social service ~every 1s; push only when something changed.
  let acc = 1
  let busy = false
  let lastKey = ''
  ctx.push((dt) => {
    acc += dt
    if (acc < 1 || busy) return
    acc = 0
    busy = true
    void poll().finally(() => {
      busy = false
    })
  })

  async function poll(): Promise<void> {
    try {
      if (!(await social.getSocialInitialized())) {
        push(false, [], [], [], [])
        return
      }
      const [online, received, sent] = await Promise.all([
        social.getOnlineFriends(),
        social.getReceivedFriendRequests(),
        social.getSentFriendRequests()
      ])
      let blocked: string[] = []
      try {
        const status = await social.getBlockingStatus?.()
        blocked = status?.blockedUsers ?? (await social.getBlockedUsers()).map((b) => b.address)
      } catch {
        /* keep empty on failure */
      }
      push(true, online, received, sent, blocked)
    } catch (e) {
      console.error('[friends] poll failed', e)
    }
  }

  function push(
    available: boolean,
    online: FriendStatusData[],
    received: FriendRequestData[],
    sent: FriendRequestData[],
    blocked: string[]
  ): void {
    const friends = online.map(toFriend)
    const recv = received.map(toRequest)
    const snt = sent.map(toRequest)
    const key = `${String(available)}|${friends.map((f) => `${f.address}${f.status}${f.picture ?? ''}`).join(',')}|${recv.map((r) => r.id).join(',')}|${snt.map((r) => r.id).join(',')}|${blocked.join(',')}`
    if (key === lastKey) return
    lastKey = key
    ctx.send({ kind: 'friends', available, friends, received: recv, sent: snt, blocked })
  }
}
