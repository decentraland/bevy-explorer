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
import type { Badge, Profile, ProfileInfo, SaveProfileRequest } from '../../../src/engine/protocol'
import type { SetAvatarData } from '../../../src/engine/generated'
// Not via the generated barrel: it only re-exports the top-level files, not serde_json/.
import type { JsonValue } from '../../../src/engine/generated/serde_json/JsonValue'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'

type CatalystAvatar = {
  name?: string
  hasClaimedName?: boolean
  /** Profile-set custom name colour (claimed names only), 0–1 floats. */
  nameColor?: { r: number; g: number; b: number }
  description?: string
  links?: Array<{ title: string; url: string }>
  // --- about-me fields. The renderer doesn't model these, so they ride the profile as free-form
  // extra keys (SerializedProfile::extra_fields) and reach us flattened onto the avatar. Names are
  // the ones unity-explorer and the old system scene write — matching them is what makes a profile
  // edited in either client read back correctly in the other.
  country?: string
  language?: string
  gender?: string
  pronouns?: string
  relationshipStatus?: string
  sexualOrientation?: string
  employmentStatus?: string
  profession?: string
  hobbies?: string
  realName?: string
  /** Epoch SECONDS (what the old scene writes), not the ISO string the passport shows. */
  birthdate?: number
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
    links: av?.links ?? undefined,
    info: toInfo(av)
  }
}

// --- about-me fields ------------------------------------------------------------
// The wire (`ProfileInfo`) names the fields as the passport labels them; the profile stores them
// under the names unity-explorer chose. One table drives both directions so a rename can't leave
// the read and the write disagreeing — which would look like an edit that silently didn't save.
const INFO_KEYS = {
  country: 'country',
  language: 'language',
  gender: 'gender',
  pronouns: 'pronouns',
  relationship: 'relationshipStatus',
  sexualOrientation: 'sexualOrientation',
  employment: 'employmentStatus',
  profession: 'profession',
  hobby: 'hobbies',
  realName: 'realName'
} as const satisfies Partial<Record<keyof ProfileInfo, keyof CatalystAvatar>>

/** Epoch seconds (how the profile stores a birthdate) → the `YYYY-MM-DD` the passport edits.
 *  UTC on both sides: a birthday is a date, and shifting it by the viewer's timezone would let a
 *  profile read back a day out from the one that was saved. */
const toIsoDate = (seconds: number | undefined): string | undefined => {
  if (typeof seconds !== 'number' || !Number.isFinite(seconds) || seconds === 0) return undefined
  const date = new Date(seconds * 1000)
  // A profile is user-supplied data: an out-of-range timestamp makes toISOString THROW rather than
  // return anything, so the date has to be checked before it's formatted.
  if (Number.isNaN(date.getTime())) return undefined
  return date.toISOString().slice(0, 10)
}

const fromIsoDate = (iso: string | undefined): number | undefined => {
  if (iso == null || iso === '') return undefined
  const ms = Date.parse(`${iso}T00:00:00Z`)
  return Number.isNaN(ms) ? undefined : Math.floor(ms / 1000)
}

/** `undefined` when the profile carries no about-me fields at all, so the passport can tell an
 *  empty section from a missing one. */
function toInfo(av: CatalystAvatar | undefined): ProfileInfo | undefined {
  if (av == null) return undefined
  const info: ProfileInfo = {}
  for (const [wireKey, profileKey] of Object.entries(INFO_KEYS) as Array<[keyof ProfileInfo, keyof CatalystAvatar]>) {
    const value = av[profileKey]
    if (typeof value === 'string' && value !== '') info[wireKey] = value
  }
  const birthdate = toIsoDate(av.birthdate)
  if (birthdate != null) info.birthdate = birthdate
  return Object.keys(info).length > 0 ? info : undefined
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

// --- claimed names -------------------------------------------------------------
// The NFT names this wallet owns. Only one of them can be worn without the `#1234` suffix, so the
// picker offers exactly these; anything else is an unclaimed name.
type NamesResponse = { elements?: Array<{ name?: string }> }

let ownedNames: string[] | undefined
async function fetchOwnedNames(address: string): Promise<string[]> {
  if (ownedNames != null) return ownedNames
  const base = await catalystBase()
  const r = await getJson<NamesResponse>(`${base}/lambdas/users/${address}/names`).catch(() => undefined)
  // Cached only on success: a failed lookup must not pin an empty picker for the whole session.
  if (r == null) return []
  ownedNames = (r.elements ?? []).map((e) => e.name).filter((n): n is string => typeof n === 'string' && n !== '')
  return ownedNames
}

/**
 * A save turns the passport's edits into the profile keys the deployed profile actually holds.
 * Everything here is a PARTIAL update — the engine merges `profileExtras` per key and treats an
 * empty `bodyShapeUrn`/absent colors as "unchanged" — so we send only what the user edited and
 * never have to restate (and risk clobbering) the rest of their profile. Clearing a field sends
 * `null`, which removes the key outright rather than leaving an empty string behind.
 */
function toProfileExtras(msg: SaveProfileRequest): Record<string, JsonValue> {
  const extras: Record<string, JsonValue> = {}
  const set = (key: string, value: string | number | undefined): void => {
    extras[key] = value == null || value === '' ? null : value
  }
  // `description` is REQUIRED by the profile schema (@dcl/schemas Avatar), so clearing it means
  // sending an empty string: a null here removes the key, and the catalyst rejects the deploy with
  // "failed to deploy to server." Every other field below is nullable/optional, so for those a
  // null — which the engine applies by removing the key — is the right way to clear.
  if (msg.description !== undefined) extras.description = msg.description.trim()
  if (msg.links !== undefined) {
    const links = msg.links.filter((l) => l.url !== '')
    extras.links = links.length > 0 ? links : null
  }
  if (msg.info !== undefined) {
    for (const [wireKey, profileKey] of Object.entries(INFO_KEYS) as Array<[keyof ProfileInfo, string]>) {
      set(profileKey, msg.info[wireKey]?.trim())
    }
    set('birthdate', fromIsoDate(msg.info.birthdate))
  }
  return extras
}

/** Fold a save into the cached catalyst avatar, so every surface that re-reads the profile shows
 *  the new values immediately. The deploy is asynchronous and the catalyst reindexes later still,
 *  so without this a passport reopened right after saving would show the pre-edit profile. */
function patchCachedAvatar(address: string, msg: SaveProfileRequest, hasClaimedName: boolean | undefined): void {
  const cached = cache.get(profileKey(address)) ?? { avatars: [{}] }
  const av = { ...((cached.avatars?.[0] ?? {}) as Record<string, unknown>) }
  if (msg.name !== undefined) {
    av.name = msg.name
    if (hasClaimedName !== undefined) av.hasClaimedName = hasClaimedName
  }
  // A `null` from toProfileExtras means "remove this key", so those are dropped rather than
  // written — the engine does the same to the profile itself.
  const patch = toProfileExtras(msg)
  const cleared = new Set(Object.keys(patch).filter((key) => patch[key] == null))
  const patched = Object.fromEntries(Object.entries({ ...av, ...patch }).filter(([key]) => !cleared.has(key)))
  cache.set(profileKey(address), { ...cached, avatars: [patched as CatalystAvatar, ...(cached.avatars ?? []).slice(1)] })
}

export function registerProfile(ctx: Ctx): void {
  ctx.on('getOwnedNames', async () => {
    const player = getPlayer()
    ctx.send({ kind: 'ownedNames', names: player == null ? [] : await fetchOwnedNames(player.userId).catch(() => []) })
  })

  ctx.on('saveProfile', async (msg) => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'profileSaved', ok: false, error: 'Not signed in yet.' })
      return
    }

    const data: SetAvatarData = {}
    let hasClaimedName: boolean | undefined
    if (msg.name !== undefined) {
      const owned = await fetchOwnedNames(player.userId).catch(() => [])
      hasClaimedName = owned.some((n) => n.toLowerCase() === msg.name?.toLowerCase())
      // An empty bodyShapeUrn and null colors mean "leave the avatar alone" — the name is the only
      // thing this edit touches, and the Backpack owns the rest.
      data.base = { skinColor: null, eyesColor: null, hairColor: null, bodyShapeUrn: '', name: msg.name }
      data.hasClaimedName = hasClaimedName
    }
    const extras = toProfileExtras(msg)
    if (Object.keys(extras).length > 0) data.profileExtras = extras
    if (data.base == null && data.profileExtras == null) {
      ctx.send({ kind: 'profileSaved', ok: true })
      return
    }

    try {
      // setAvatar resolves only once the engine has deployed the new profile version (guests
      // resolve immediately — they have no catalyst presence to deploy to), so a rejection here
      // is a genuinely failed save and the HUD must not keep showing the edit as if it stuck.
      await BevyApi.setAvatar(data)
    } catch (e) {
      console.error('[profile] save failed', e)
      ctx.send({ kind: 'profileSaved', ok: false, error: e instanceof Error ? e.message : String(e) })
      return
    }

    patchCachedAvatar(player.userId, msg, hasClaimedName)
    ctx.send({ kind: 'profileSaved', ok: true })
    const av = cache.get(profileKey(player.userId))?.avatars?.[0]
    ctx.send({ kind: 'profile', profile: toProfile(av, player.userId, player.isGuest, player.name) })
  })

  // The page marks its world-entry profile fetch done as soon as it ASKS, so an answer of `null`
  // costs it the profile for the whole session. `getPlayer()` is the scene's view of the player
  // CRDT, which lags world entry by a good few hundred frames, so a request that arrives in that
  // window is held rather than answered — the page only ever hears a profile it can use.
  let wanted = false
  let inFlight = false
  const answerProfile = async (): Promise<void> => {
    const player = getPlayer()
    if (player == null || inFlight) return
    wanted = false
    inFlight = true
    try {
      const data = await fetchProfile(player.userId).catch(() => undefined)
      ctx.send({ kind: 'profile', profile: toProfile(data?.avatars?.[0], player.userId, player.isGuest, player.name) })
    } finally {
      inFlight = false
    }
  }
  ctx.on('getProfile', () => {
    wanted = true
    void answerProfile()
  })
  ctx.push(() => {
    if (wanted) void answerProfile()
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
