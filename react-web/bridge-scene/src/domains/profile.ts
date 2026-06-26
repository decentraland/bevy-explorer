// Profile: the local player's profile card + any user's passport (View Profile).
//   from: @dcl/sdk getPlayer() (address/name/isGuest)
//       + catalyst lambda  GET /lambdas/profiles/:userId  (avatar face + body, name, links)
//       + badges service   GET badges.decentraland.org/users/:id/badges
//       + camera-reel       GET camera-reel-service.decentraland.org/api/users/:id/images
import { getPlayer } from '@dcl/sdk/players'
import { catalystBase, getJson } from '../http'
import type { Badge, Profile } from '../../../src/engine/protocol'
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

const httpOrUndef = (s?: string | null): string | undefined => (typeof s === 'string' && s.startsWith('http') ? s : undefined)

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

// --- badges (achieved only) ----------------------------------------------------
type BadgesResponse = {
  data?: {
    achieved?: Array<{
      id: string
      name: string
      assets?: { '2d'?: Partial<Record<string, string>> }
      progress?: { lastCompletedTierName?: string | null; lastCompletedTierImage?: string | null }
    }>
  }
}
async function fetchBadges(address: string): Promise<Badge[] | undefined> {
  const r = await getJson<BadgesResponse>(`https://badges.decentraland.org/users/${address}/badges`).catch(() => undefined)
  const achieved = r?.data?.achieved
  if (achieved == null) return undefined
  return achieved.map((b) => ({
    id: b.id,
    name: b.name,
    tier: b.progress?.lastCompletedTierName ?? undefined,
    image: httpOrUndef(b.progress?.lastCompletedTierImage) ?? httpOrUndef(b.assets?.['2d']?.normal)
  }))
}

// --- camera-reel photos --------------------------------------------------------
type ReelResponse = { images?: Array<{ url?: string; thumbnailUrl?: string }> }
async function fetchPhotos(address: string): Promise<string[] | undefined> {
  const r = await getJson<ReelResponse>(
    `https://camera-reel-service.decentraland.org/api/users/${address}/images?limit=12&offset=0&compact=true`
  ).catch(() => undefined)
  const imgs = r?.images
  if (imgs == null) return undefined
  return imgs.map((i) => i.thumbnailUrl ?? i.url).filter((u): u is string => typeof u === 'string')
}

export function registerProfile(ctx: Ctx): void {
  ctx.on('getProfile', async () => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'profile', profile: null })
      return
    }
    const data = await fetchProfile(player.userId).catch(() => undefined)
    ctx.send({ kind: 'profile', profile: toProfile(data?.avatars?.[0], player.userId, player.isGuest, player.name) })
  })

  // View Profile: fetch another user's full passport by address (profile + badges + photos).
  ctx.on('getUserProfile', async (msg) => {
    const [data, badges, photos] = await Promise.all([
      fetchProfile(msg.address).catch(() => undefined),
      fetchBadges(msg.address).catch(() => undefined),
      fetchPhotos(msg.address).catch(() => undefined)
    ])
    const av = data?.avatars?.[0]
    if (av == null && badges == null && photos == null) {
      ctx.send({ kind: 'userProfile', address: msg.address, profile: null })
      return
    }
    ctx.send({
      kind: 'userProfile',
      address: msg.address,
      profile: { ...toProfile(av, msg.address, false, av?.name ?? msg.address), badges, photos }
    })
  })
}
