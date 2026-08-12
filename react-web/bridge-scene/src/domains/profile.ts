// Profile: the local player's profile card + any user's passport (View Profile).
//   from: @dcl/sdk getPlayer() (address/name/isGuest)
//       + catalyst lambda  GET /lambdas/profiles/:userId  (avatar face + body, name, links)
//       + badges service   GET badges.decentraland.org/users/:id/badges
//       + camera-reel       GET camera-reel-service.decentraland.org/api/users/:id/images
import { getPlayer } from '@dcl/sdk/players'
import { catalystBase, getJson } from '../http'
import { resolveEquippedSet } from './wearables'
import { equippedSlots, resolveEquippedEmotes } from './emotes'
import type { Badge, Profile } from '../../../src/engine/protocol'
import type { Ctx } from '../bridge'

type CatalystAvatar = {
  name?: string
  hasClaimedName?: boolean
  /** Profile-set custom name colour (claimed names only), 0–1 floats. */
  nameColor?: { r: number; g: number; b: number }
  description?: string
  links?: Array<{ title: string; url: string }>
  avatar?: {
    snapshots?: { face256?: string; body?: string }
    /** Deployed equipped-wearables urns — resolved into the passport's Equipped Wearables section. */
    wearables?: string[]
    /** Deployed equipped-emotes wheel slots — resolved into the passport's Equipped Emotes section. */
    emotes?: Array<{ slot: number; urn: string }>
  }
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
    // Your OWN passport: read the live avatar (getPlayer()) rather than the deployed catalyst
    // profile, so it matches the Backpack exactly — a just-equipped item shows immediately instead
    // of waiting for the profile to redeploy and the catalyst to reindex it. Other users have no
    // live source (getPlayer(userId) only resolves nearby avatars), so they stay catalyst-only.
    const me = getPlayer()
    const isSelf = me != null && me.userId.toLowerCase() === msg.address.toLowerCase()
    const wearableUrns = isSelf ? (me.wearables ?? []).map(String) : (av?.avatar?.wearables ?? [])
    // Through equippedSlots, not me.emotes: the bevy runtime leaves a fresh profile's wheel empty
    // and the emote wheel fills it with the 10 base emotes, so reading the raw array would show an
    // empty Equipped Emotes section in your own passport while the wheel shows ten.
    const emoteEntries = isSelf
      ? equippedSlots(me.emotes).map((urn, slot) => ({ slot, urn }))
      : (av?.avatar?.emotes ?? [])
    const [equippedWearables, equippedEmotes] = await Promise.all([
      // indexTokens only for our OWN urns: another user's tokenIds must never reach the map the
      // equip handler deploys from. shopUrls because the passport is the only surface that renders
      // a SHOP action (see resolveEquippedSet).
      resolveEquippedSet(wearableUrns, { indexTokens: isSelf, shopUrls: true }).catch(() => undefined),
      resolveEquippedEmotes(emoteEntries).catch(() => undefined)
    ])
    ctx.send({
      kind: 'userProfile',
      address: msg.address,
      profile: { ...toProfile(av, msg.address, false, av?.name ?? msg.address), badges, photos, equippedWearables, equippedEmotes }
    })
  })
}
