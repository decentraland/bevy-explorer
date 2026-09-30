// Friends: list, requests, blocked, and friend actions.
//   from: BevyApi.social.* (the engine's authenticated social-service client).
// No store: we poll the social API directly (~every 1s) and push on change, so the whole
// friends surface is right here in one file.
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi, type FriendStatusData, type FriendRequestData } from '../bevy-api'
import type { Ctx } from '../bridge'
import type { BlockedUser, Friend, FriendRequest } from '../../../src/engine/protocol'
import type { BlockedUserData } from '../../../src/engine/generated'

const toFriend = (f: FriendStatusData): Friend => ({
  address: f.address,
  name: f.name,
  picture: f.profilePictureUrl !== '' ? f.profilePictureUrl : undefined,
  // the engine only emits "online" | "offline" | "away" (generated type widens to string)
  status: f.status as Friend['status'],
  claimed: f.hasClaimedName,
  nameColor: f.nameColor ?? undefined
})
const toRequest = (r: FriendRequestData): FriendRequest => ({
  address: r.address,
  name: r.name,
  picture: r.profilePictureUrl !== '' ? r.profilePictureUrl : undefined,
  message: r.message ?? undefined,
  id: r.id,
  createdAt: r.createdAt,
  claimed: r.hasClaimedName,
  nameColor: r.nameColor ?? undefined
})
const toBlocked = (b: BlockedUserData): BlockedUser => ({
  address: b.address,
  name: b.name,
  picture: b.profilePictureUrl !== '' ? b.profilePictureUrl : undefined,
  claimed: b.hasClaimedName,
  nameColor: b.nameColor ?? undefined
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
      ctx.send({ kind: 'friendActionFailed', op: msg.op, address: a, error: String(e) })
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
        const me = getPlayer()
        push(false, [], [], [], [], [], me != null && !me.isGuest)
        return
      }
      const [online, received, sent] = await Promise.all([
        social.getOnlineFriends(),
        social.getReceivedFriendRequests(),
        social.getSentFriendRequests()
      ])
      let blocked: string[] = []
      let blockedUsers: BlockedUserData[] = []
      try {
        blockedUsers = await social.getBlockedUsers()
        blocked = blockedUsers.map((b) => b.address)
      } catch {
        try {
          blocked = (await social.getBlockingStatus?.())?.blockedUsers ?? []
        } catch {
          /* keep empty on failure */
        }
      }
      push(true, online, received, sent, blocked, blockedUsers, false)
    } catch (e) {
      console.error('[friends] poll failed', e)
    }
  }

  function push(
    available: boolean,
    online: FriendStatusData[],
    received: FriendRequestData[],
    sent: FriendRequestData[],
    blocked: string[],
    blockedUsers: BlockedUserData[],
    loading: boolean
  ): void {
    const friends = online.map(toFriend)
    const recv = received.map(toRequest)
    const snt = sent.map(toRequest)
    const key = `${String(available)}|${String(loading)}|${blockedUsers.map((b) => b.address + b.name + b.profilePictureUrl).join(',')}|${friends.map((f) => `${f.address}${f.status}${f.picture ?? ''}`).join(',')}|${recv.map((r) => r.id).join(',')}|${snt.map((r) => r.id).join(',')}|${blocked.join(',')}`
    if (key === lastKey) return
    lastKey = key
    ctx.send({ kind: 'friends', available, loading, friends, received: recv, sent: snt, blocked, blockedUsers: blockedUsers.map(toBlocked) })
  }
}
