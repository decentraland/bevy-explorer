// Profile: the local player's profile card.
//   from: @dcl/sdk getPlayer() (address/name/isGuest) + catalyst lambda
//         GET /lambdas/profiles/:userId (avatar face, claimed name, description, links).
import { getPlayer } from '@dcl/sdk/players'
import { catalystBase, getJson } from '../http'
import type { Ctx } from '../bridge'

type CatalystAvatar = {
  name?: string
  hasClaimedName?: boolean
  description?: string
  links?: Array<{ title: string; url: string }>
  avatar?: { snapshots?: { face256?: string } }
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

export function registerProfile(ctx: Ctx): void {
  ctx.on('getProfile', async () => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'profile', profile: null })
      return
    }
    const data = await fetchProfile(player.userId).catch(() => undefined)
    const av = data?.avatars?.[0]
    const face = av?.avatar?.snapshots?.face256
    ctx.send({
      kind: 'profile',
      profile: {
        address: player.userId,
        // Prefer the catalyst profile name; getPlayer().name can be the engine's default
        // ("Bevy_User") when the engine hasn't loaded the deployed profile.
        name: av?.name != null && av.name !== '' ? av.name : player.name,
        picture: typeof face === 'string' && face.startsWith('http') ? face : undefined,
        hasClaimedName: av?.hasClaimedName ?? !player.name.includes('#'),
        isGuest: player.isGuest,
        description: av?.description != null && av.description !== '' ? av.description : undefined,
        links: av?.links ?? undefined
      }
    })
  })
}
