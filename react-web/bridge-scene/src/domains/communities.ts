// Communities: browse list, joining, and per-community detail (members/posts/places).
//   from: social-api communities service via BevyApi.kernelFetch (signed GETs / POST join)
//       + the engine's profile cache (profile.ts) for the names and faces those reads no
//         longer carry — the v2 endpoints answer with addresses only.
import { getPlayer } from '@dcl/sdk/players'
import { getJson, isZone, signed, signedForm } from '../http'
import { fetchIdentities } from './profile'
import type { ProfileIdentity } from './profile'
import type { Ctx } from '../bridge'
import type { Community, CommunityEvent, CommunityMember, CommunityPhoto, CommunityPlace, CommunityPost } from '../../../src/engine/protocol'

const ORG = 'https://social-api.decentraland.org'
const ZONE = 'https://social-api.decentraland.zone'
// Community events live on the (public) events-api, filtered by community_id.
const EVENTS_ORG = 'https://events.decentraland.org/api/events'
const EVENTS_ZONE = 'https://events.decentraland.zone/api/events'
// Community photos live on the camera-reel service (mirrors /api/users/{a}/images).
const REEL_ORG = 'https://camera-reel-service.decentraland.org/api/communities'
const REEL_ZONE = 'https://camera-reel-service.decentraland.zone/api/communities'
// Community thumbnails are NOT in the list response — Unity builds them from the id against
// the assets CDN (DecentralandUrl.CommunityThumbnail). Some 404 (no thumbnail set) → the
// React <img> falls back to the initial.
const CDN_ORG = 'https://assets-cdn.decentraland.org'
const CDN_ZONE = 'https://assets-cdn.decentraland.zone'

type CommunityRaw = {
  id: string
  name: string
  description: string
  membersCount: number
  role: string
  ownerAddress?: string
  privacy?: string
}

// The v2 rows are FLAT and address-only: no name, face, or claimed-name flag on any of them.
type MemberRaw = { memberAddress?: string; role?: string; friendshipStatus?: number }
type PostRaw = { id: string; authorAddress?: string; content?: string; createdAt?: string; likesCount?: number }
type PlaceRaw = { id: string; title?: string; name?: string; image?: string; base_position?: string; positions?: string[]; like_rate?: number; likeRate?: number }
type EventRaw = { id: string; name?: string; image?: string; next_start_at?: string; start_at?: string }
type PhotoRaw = { id: string; url?: string; thumbnailUrl?: string }

// FriendshipStatus enum: request_sent=0 … friend=3 … none=7. Hide "Add Friend" once friends.
const FRIEND = 3

// Reads take the v2 endpoints, which answer with addresses instead of embedding a profile the
// service had to resolve per row. Writes (and /places, which carries no profile) have no v2.
async function base(version: 'v1' | 'v2'): Promise<string> {
  return `${(await isZone()) ? ZONE : ORG}/${version}/communities`
}

async function list(): Promise<Community[]> {
  const cdn = (await isZone()) ? CDN_ZONE : CDN_ORG
  const res = await signed<{ results?: CommunityRaw[] }>(`${await base('v2')}?limit=50`)
  const raw = res?.results ?? []
  const identity = await fetchIdentities(raw.map((c) => c.ownerAddress ?? ''))
  return raw.map((c) => ({
    id: c.id,
    name: c.name,
    description: c.description,
    thumbnail: `${cdn}/social/communities/${c.id}/raw-thumbnail.png`,
    membersCount: c.membersCount,
    role: c.role,
    ownerName: identity(c.ownerAddress ?? '').name,
    privacy: c.privacy
  }))
}

function mapMember(m: MemberRaw, identity: (address: string) => ProfileIdentity): CommunityMember {
  const address = m.memberAddress ?? ''
  const { name, picture, hasClaimedName } = identity(address)
  return {
    address,
    name,
    role: m.role ?? 'member',
    picture,
    hasClaimedName,
    isFriend: m.friendshipStatus === FRIEND
  }
}

async function detail(id: string): Promise<{ members: CommunityMember[]; posts: CommunityPost[]; places: CommunityPlace[]; events: CommunityEvent[]; photos: CommunityPhoto[] }> {
  const b = await base('v2')
  const placesBase = await base('v1')
  const zone = await isZone()
  const eventsBase = zone ? EVENTS_ZONE : EVENTS_ORG
  const reelBase = zone ? REEL_ZONE : REEL_ORG
  // The id reaches us from the service (a uuid), but it lands in a URL — encode it so it
  // can only ever be one path segment / one query value.
  const cid = encodeURIComponent(id)
  const [membersRes, postsRes, placesRes, eventsRes, photosRes] = await Promise.all([
    signed<{ results?: MemberRaw[] }>(`${b}/${cid}/members?limit=100`).catch(() => undefined),
    signed<{ posts?: PostRaw[] }>(`${b}/${cid}/posts?limit=20`).catch(() => undefined),
    signed<{ results?: PlaceRaw[] }>(`${placesBase}/${cid}/places?limit=20`).catch(() => undefined),
    getJson<{ data?: { events?: EventRaw[] } }>(`${eventsBase}?community_id=${cid}&list=upcoming`).catch(() => undefined),
    signed<{ images?: PhotoRaw[] }>(`${reelBase}/${cid}/images?limit=30`).catch(() => undefined)
  ])
  const memberRows = membersRes?.results ?? []
  const postRows = postsRes?.posts ?? []
  // One batch for every address both tabs reference — the members list alone would otherwise
  // be up to 100 separate profile fetches.
  const identity = await fetchIdentities([
    ...memberRows.map((m) => m.memberAddress ?? ''),
    ...postRows.map((p) => p.authorAddress ?? '')
  ])
  const members = memberRows.map((m) => mapMember(m, identity))
  const posts: CommunityPost[] = postRows.map((p) => {
    const address = p.authorAddress ?? ''
    const { name, picture } = identity(address)
    return {
      id: p.id,
      author: name,
      authorAddress: address,
      authorPicture: picture,
      text: p.content ?? '',
      timestamp: p.createdAt != null ? Date.parse(p.createdAt) : 0,
      likes: p.likesCount ?? 0
    }
  })
  const places: CommunityPlace[] = (placesRes?.results ?? []).map((pl) => ({
    id: pl.id,
    title: pl.title ?? pl.name ?? '',
    thumbnail: pl.image,
    positions: pl.base_position ?? pl.positions?.[0],
    likeRate: pl.like_rate ?? pl.likeRate
  }))
  const events: CommunityEvent[] = (eventsRes?.data?.events ?? []).map((e) => ({
    id: e.id,
    name: e.name ?? '',
    thumbnail: e.image,
    startsAt: Date.parse(e.next_start_at ?? e.start_at ?? '') || 0
  }))
  const photos: CommunityPhoto[] = (photosRes?.images ?? []).filter((ph) => ph.url != null).map((ph) => ({
    id: ph.id,
    url: ph.url ?? '',
    thumbnail: ph.thumbnailUrl ?? ph.url
  }))
  return { members, posts, places, events, photos }
}

export function registerCommunities(ctx: Ctx): void {
  ctx.on('getCommunities', async () => {
    ctx.send({ kind: 'communities', communities: await list() })
  })
  ctx.on('createCommunity', async (msg) => {
    // Text-only multipart (no thumbnail — see signedForm). Matches Unity's create payload.
    await signedForm(await base('v1'), 'POST', {
      name: msg.name,
      description: msg.description,
      privacy: msg.privacy,
      visibility: msg.discoverable ? 'all' : 'unlisted'
    }).catch((e: unknown) => {
      console.error('[communities] create failed', e)
    })
    ctx.send({ kind: 'communities', communities: await list() })
  })
  ctx.on('joinCommunity', async (msg) => {
    await signed(`${await base('v1')}/${encodeURIComponent(msg.id)}/members`, 'POST')
    ctx.send({ kind: 'communities', communities: await list() })
  })
  ctx.on('leaveCommunity', async (msg) => {
    const me = getPlayer()?.userId
    if (me != null && me !== '') {
      await signed(`${await base('v1')}/${encodeURIComponent(msg.id)}/members/${encodeURIComponent(me)}`, 'DELETE').catch(() => undefined)
    }
    ctx.send({ kind: 'communities', communities: await list() })
  })
  ctx.on('getCommunityDetail', async (msg) => {
    const { members, posts, places, events, photos } = await detail(msg.id).catch(() => ({ members: [], posts: [], places: [], events: [], photos: [] }))
    ctx.send({ kind: 'communityDetail', id: msg.id, members, posts, places, events, photos })
  })
}
