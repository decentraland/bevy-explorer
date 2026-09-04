// Profile: the local player's profile card + any user's passport (View Profile).
//   from: @dcl/sdk getPlayer() (address/name/isGuest)
//       + catalyst lambda  GET /lambdas/profiles/:userId  (avatar face + body, name, links)
//       + the ENGINE's profile cache via ~system/Players getPlayerData (display identity for a list)
//       + badges service   GET badges.decentraland.org/users/:id/badges
//       + camera-reel       GET camera-reel-service.decentraland.org/api/users/:id/images
import { getPlayer } from '@dcl/sdk/players'
import { getPlayerData } from '~system/Players'
import { catalystBase, getJson } from '../http'
import type { UserData } from '~system/Players'
import { resolveEquippedSet, resolveWearables } from './wearables'
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

/** Addresses are the cache key, always lowercased — the same wallet reaches us in either case. */
export const profileKey = (address: string): string => address.toLowerCase()

const cache = new Map<string, ProfileResponse>()
export { cache as profileCache }

export async function fetchProfile(userId: string): Promise<ProfileResponse | undefined> {
  const cached = cache.get(profileKey(userId))
  if (cached != null) return cached
  const base = await catalystBase()
  const data = await getJson<ProfileResponse>(`${base}/lambdas/profiles/${userId}`).catch(() => undefined)
  if (data != null) cache.set(profileKey(userId), data)
  return data
}

/** What a list row needs to show a person: their name, face, and claimed-name seal. */
export type ProfileIdentity = { name: string; picture?: string; hasClaimedName: boolean }

/** `hasClaimedName` rides the same payload, but isn't in the SDK's `UserData` typing. */
type PlayerData = UserData & { hasClaimedName?: boolean }

/**
 * Ceiling on one lookup. Each address costs a slot in the scene's per-tick RPC budget
 * (1000, shared with every other call the bridge makes that tick), so a service that
 * ignores its `limit` param has to cost a truncated list rather than a broken tick. The
 * real callers ask for at most 120 — 100 members plus 20 post authors. Anything dropped
 * still renders, as its shortened address.
 */
const MAX_IDENTITIES = 200

/**
 * Resolve display identity for a list of addresses through the ENGINE's profile cache —
 * the one the nametags, chat and passport UI already read. `getPlayerData` asks about one
 * address, but the engine batches every address it doesn't already hold into a single
 * registry request, so asking about a whole list at once still costs one round trip and
 * anyone already on screen costs nothing. Returns a lookup that always answers: an address
 * with no (or an unresolvable) profile falls back to its shortened form.
 */
export async function fetchIdentities(addresses: string[]): Promise<(address: string) => ProfileIdentity> {
  const unique = [...new Set(addresses.filter((a) => a !== '').map(profileKey))]
  if (unique.length > MAX_IDENTITIES) console.error(`[profile] ${unique.length} addresses asked for, resolving the first ${MAX_IDENTITIES}`)
  const wanted = unique.slice(0, MAX_IDENTITIES)
  const resolved = new Map<string, PlayerData>()
  await Promise.all(
    wanted.map(async (address) => {
      const data = await getPlayerData({ userId: address })
        .then((res) => res.data)
        .catch(() => undefined)
      if (data != null) resolved.set(address, data)
    })
  )
  return (address) => identityOf(resolved.get(profileKey(address)), address)
}

function identityOf(data: PlayerData | undefined, address: string): ProfileIdentity {
  return {
    name: data?.displayName != null && data.displayName !== '' ? data.displayName : shortAddress(address),
    picture: httpOrUndef(data?.avatar?.snapshots?.face256),
    hasClaimedName: data?.hasClaimedName ?? false
  }
}

const shortAddress = (a: string): string => (a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a)

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
      // Only our OWN urns go through resolveEquippedSet (it indexes tokens for the equip handler);
      // shopUrls because the passport is the only surface that renders a SHOP action.
      (isSelf ? resolveEquippedSet : resolveWearables)(wearableUrns, { shopUrls: true }).catch(() => undefined),
      resolveEquippedEmotes(emoteEntries).catch(() => undefined)
    ])
    ctx.send({
      kind: 'userProfile',
      address: msg.address,
      profile: { ...toProfile(av, msg.address, false, av?.name ?? msg.address), badges, photos, equippedWearables, equippedEmotes }
    })
  })
}
