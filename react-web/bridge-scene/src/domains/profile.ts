// Profile: the local player's profile card + any user's passport (View Profile).
//   from: @dcl/sdk getPlayer() (address/name/isGuest) + catalyst lambda
//         GET /lambdas/profiles/:userId (avatar face + body, claimed name, description, links).
import { getPlayer } from '@dcl/sdk/players'
import { catalystBase, getJson } from '../http'
import type { Profile } from '../../../src/engine/protocol'
import type { Ctx } from '../bridge'

type CatalystAvatar = {
  name?: string
  hasClaimedName?: boolean
  description?: string
  links?: Array<{ title: string; url: string }>
  avatar?: { snapshots?: { face256?: string; body?: string } }
}
export type ProfileResponse = { avatars?: CatalystAvatar[] }

const cache = new Map<string, ProfileResponse>()
export { cache as profileCache }

export async function fetchProfile(userId: string): Promise<ProfileResponse | undefined> {
  const cached = cache.get(userId)
  if (cached != null) return cached
  const base = await catalystBase()
  const data = await getJson<ProfileResponse>(`${base}/lambdas/profiles/${userId}`).catch(() => undefined)
  if (data != null) cache.set(userId, data)
  return data
}

const httpOrUndef = (s?: string): string | undefined => (typeof s === 'string' && s.startsWith('http') ? s : undefined)

// Map a catalyst avatar record to the wire Profile. The rich passport fields (badges/info)
// aren't in the base catalyst profile, so they stay undefined (the passport hides them).
function toProfile(av: CatalystAvatar | undefined, address: string, isGuest: boolean, fallbackName: string): Profile {
  const snaps = av?.avatar?.snapshots
  return {
    address,
    name: av?.name != null && av.name !== '' ? av.name : fallbackName,
    picture: httpOrUndef(snaps?.face256),
    bodyImage: httpOrUndef(snaps?.body),
    hasClaimedName: av?.hasClaimedName ?? !fallbackName.includes('#'),
    isGuest,
    description: av?.description != null && av.description !== '' ? av.description : undefined,
    links: av?.links ?? undefined
  }
}

export function registerProfile(ctx: Ctx): void {
  ctx.on('getProfile', async () => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'profile', profile: null })
      return
    }
    // Prefer the catalyst name; getPlayer().name can be the engine default ("Bevy_User")
    // before the deployed profile loads.
    const data = await fetchProfile(player.userId).catch(() => undefined)
    ctx.send({ kind: 'profile', profile: toProfile(data?.avatars?.[0], player.userId, player.isGuest, player.name) })
  })

  // View Profile: fetch another user's passport by address (name/picture/body/links).
  ctx.on('getUserProfile', async (msg) => {
    const data = await fetchProfile(msg.address).catch(() => undefined)
    const av = data?.avatars?.[0]
    ctx.send({
      kind: 'userProfile',
      address: msg.address,
      profile: av == null ? null : toProfile(av, msg.address, false, av.name ?? msg.address)
    })
  })
}
