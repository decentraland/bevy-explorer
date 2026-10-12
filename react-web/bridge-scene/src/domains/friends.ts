// Friends: list, requests, blocked, and friend actions.
//   from: BevyApi.social.* (the engine's authenticated social-service client).
// No store: we poll the social API directly (~every 1s) and push on change, so the whole
// friends surface is right here in one file.
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi, type FriendStatusData, type FriendRequestData } from '../bevy-api'
import { relay } from '../system-helpers'
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
      msg.op === 'request' ? social.sendFriendRequest(a, msg.message)
        : msg.op === 'accept' ? social.acceptFriendRequest(a)
          : msg.op === 'reject' ? social.rejectFriendRequest(a)
            : msg.op === 'cancel' ? social.cancelFriendRequest(a)
              : msg.op === 'delete' ? social.deleteFriend(a)
                : msg.op === 'block' ? social.blockUser(a)
                  : social.unblockUser(a)
    run.then(
      () => {
        ctx.send({ kind: 'friendActionDone', op: msg.op, address: a })
        if (msg.op === 'block' || msg.op === 'unblock') blockedStale = true
        acc = 1 // refresh on the next frame instead of waiting for the poll
      },
      (e: unknown) => {
        console.error('[friends] action failed', e)
        ctx.send({ kind: 'friendActionFailed', op: msg.op, address: a, error: String(e) })
      }
    )
  })

  ctx.on('getMutualFriends', async (msg) => {
    const list = await social.getMutualFriends(msg.address).catch(() => [])
    ctx.send({
      kind: 'mutualFriends',
      address: msg.address,
      friends: list.map((f) => ({ address: f.address, name: f.name, picture: f.profilePictureUrl !== '' ? f.profilePictureUrl : undefined }))
    })
  })

  // Poll the social service ~every 1s; push only when something changed. The service's event
  // streams trigger an immediate refresh; the poll stays as the fallback.
  let acc = 1
  let elapsed = 0
  let availableSince: number | null = null
  const refresh = (): void => {
    acc = 1
  }
  relay('friendship', async () => await social.getFriendshipEventStream(), refresh)
  relay('friendConnectivity', async () => await social.getFriendConnectivityStream(), refresh)
  relay('blockUpdates', async () => await social.getBlockUpdateStream(), () => {
    blockedStale = true
    refresh()
  })
  let busy = false
  let lastKey = ''
  let blockedStale = true
  let blocked: string[] = []
  let blockedUsers: BlockedUserData[] = []
  ctx.push((dt) => {
    elapsed += dt
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
        // Presence restarts with the connection: everyone reads offline until connectivity events
        // arrive, so forget the old statuses and restart the grace period when it comes back.
        lastStatus.clear()
        availableSince = null
        blockedStale = true
        const me = getPlayer()
        push(false, [], [], [], [], [], me != null && !me.isGuest)
        return
      }
      const [online, received, sent] = await Promise.all([
        social.getOnlineFriends(),
        social.getReceivedFriendRequests(),
        social.getSentFriendRequests()
      ])
      // The blocked list is a network call to the social service (the rest is the engine's
      // in-memory copy), so it's fetched only when it can have changed, not on every poll.
      if (blockedStale) {
        blockedStale = false
        try {
          blockedUsers = await social.getBlockedUsers()
          blocked = blockedUsers.map((b) => b.address)
        } catch {
          blockedStale = true
        }
      }
      push(true, online, received, sent, blocked, blockedUsers, false)
    } catch (e) {
      console.error('[friends] poll failed', e)
    }
  }

  const lastStatus = new Map<string, string>()
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
    // "X is online" only for real transitions, not for everyone already online when the service starts.
    if (available) availableSince ??= elapsed
    const settled = availableSince != null && elapsed - availableSince > 5
    for (const f of friends) {
      const was = lastStatus.get(f.address)
      if (settled && was === 'offline' && f.status === 'online') ctx.send({ kind: 'friendOnline', address: f.address })
      lastStatus.set(f.address, f.status)
    }
    const recv = received.map(toRequest)
    const snt = sent.map(toRequest)
    const key = `${String(available)}|${String(loading)}|${blockedUsers.map((b) => b.address + b.name + b.profilePictureUrl).join(',')}|${friends.map((f) => `${f.address}${f.status}${f.picture ?? ''}`).join(',')}|${recv.map((r) => r.id).join(',')}|${snt.map((r) => r.id).join(',')}|${blocked.join(',')}`
    if (key === lastKey) return
    lastKey = key
    ctx.send({ kind: 'friends', available, loading, friends, received: recv, sent: snt, blocked, blockedUsers: blockedUsers.map(toBlocked) })
  }
}
