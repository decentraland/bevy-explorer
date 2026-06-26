// Communities: browse list, joining, and per-community detail (members/posts/places).
//   from: social-api communities service via BevyApi.kernelFetch (signed GETs / POST join).
import { getPlayer } from '@dcl/sdk/players'
import { getJson, isZone, signed } from '../http'
import type { Ctx } from '../bridge'
import type { Community, CommunityEvent, CommunityMember, CommunityPhoto, CommunityPlace, CommunityPost } from '../../../src/engine/protocol'

const ORG = 'https://social-api.decentraland.org/v1/communities'
const ZONE = 'https://social-api.decentraland.zone/v1/communities'
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
  ownerName: string
  privacy?: string
}

// The social-api returns FLAT member rows (not nested in a profile object).
type MemberRaw = { memberAddress?: string; name?: string; role?: string; profilePictureUrl?: string; hasClaimedName?: boolean; friendshipStatus?: number }
type PostRaw = { id: string; authorAddress?: string; authorName?: string; authorProfilePictureUrl?: string; content?: string; createdAt?: string; likesCount?: number }
type PlaceRaw = { id: string; title?: string; name?: string; image?: string; base_position?: string; positions?: string[]; like_rate?: number; likeRate?: number }
type EventRaw = { id: string; name?: string; image?: string; next_start_at?: string; start_at?: string }
type PhotoRaw = { id: string; url?: string; thumbnailUrl?: string }

// FriendshipStatus enum: request_sent=0 … friend=3 … none=7. Hide "Add Friend" once friends.
const FRIEND = 3

async function base(): Promise<string> {
  return (await isZone()) ? ZONE : ORG
}

async function list(): Promise<Community[]> {
  const cdn = (await isZone()) ? CDN_ZONE : CDN_ORG
  const res = await signed<{ results?: CommunityRaw[] }>(`${await base()}?limit=50`)
  return (res?.results ?? []).map((c) => ({
    id: c.id,
    name: c.name,
    description: c.description,
    thumbnail: `${cdn}/social/communities/${c.id}/raw-thumbnail.png`,
    membersCount: c.membersCount,
    role: c.role,
    ownerName: c.ownerName,
    privacy: c.privacy
  }))
}

function mapMember(m: MemberRaw): CommunityMember {
  return {
    address: m.memberAddress ?? '',
    name: m.name ?? '',
    role: m.role ?? 'member',
    picture: m.profilePictureUrl,
    hasClaimedName: m.hasClaimedName ?? false,
    isFriend: m.friendshipStatus === FRIEND
  }
}

async function detail(id: string): Promise<{ members: CommunityMember[]; posts: CommunityPost[]; places: CommunityPlace[]; events: CommunityEvent[]; photos: CommunityPhoto[] }> {
  const b = await base()
  const zone = await isZone()
  const eventsBase = zone ? EVENTS_ZONE : EVENTS_ORG
  const reelBase = zone ? REEL_ZONE : REEL_ORG
  const [membersRes, postsRes, placesRes, eventsRes, photosRes] = await Promise.all([
    signed<{ results?: MemberRaw[] }>(`${b}/${id}/members?limit=100`).catch(() => undefined),
    signed<{ posts?: PostRaw[] }>(`${b}/${id}/posts?limit=20`).catch(() => undefined),
    signed<{ results?: PlaceRaw[] }>(`${b}/${id}/places?limit=20`).catch(() => undefined),
    getJson<{ data?: { events?: EventRaw[] } }>(`${eventsBase}?community_id=${id}&list=upcoming`).catch(() => undefined),
    signed<{ images?: PhotoRaw[] }>(`${reelBase}/${id}/images?limit=30`).catch(() => undefined)
  ])
  const members = (membersRes?.results ?? []).map(mapMember)
  const posts: CommunityPost[] = (postsRes?.posts ?? []).map((p) => ({
    id: p.id,
    author: p.authorName ?? '',
    authorAddress: p.authorAddress ?? '',
    authorPicture: p.authorProfilePictureUrl,
    text: p.content ?? '',
    timestamp: p.createdAt != null ? Date.parse(p.createdAt) : 0,
    likes: p.likesCount ?? 0
  }))
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
  ctx.on('joinCommunity', async (msg) => {
    await signed(`${await base()}/${msg.id}/members`, 'POST')
    ctx.send({ kind: 'communities', communities: await list() })
  })
  ctx.on('leaveCommunity', async (msg) => {
    const me = getPlayer()?.userId
    if (me != null && me !== '') await signed(`${await base()}/${msg.id}/members/${me}`, 'DELETE').catch(() => undefined)
    ctx.send({ kind: 'communities', communities: await list() })
  })
  ctx.on('getCommunityDetail', async (msg) => {
    const { members, posts, places, events, photos } = await detail(msg.id).catch(() => ({ members: [], posts: [], places: [], events: [], photos: [] }))
    ctx.send({ kind: 'communityDetail', id: msg.id, members, posts, places, events, photos })
  })
}
