// Gallery (camera reel): the local player's in-world photos + per-photo metadata.
//   from: camera-reel-service via BevyApi.kernelFetch (signed) —
//     GET  /api/users/:address/images?compact=true   the grid (compact list + storage)
//     GET  /api/images/:id/metadata                  one photo's place + people (detail view)
//     DELETE /api/images/:id                          remove a photo (own only)
import { getPlayer } from '@dcl/sdk/players'
import { isZone, signed } from '../http'
import type { GalleryPhoto, GalleryPhotoMeta } from '../../../src/engine/protocol'
import type { Ctx } from '../bridge'

const ORG = 'https://camera-reel-service.decentraland.org/api'
const ZONE = 'https://camera-reel-service.decentraland.zone/api'

async function base(): Promise<string> {
  return (await isZone()) ? ZONE : ORG
}

type CompactImage = { id: string; url?: string; thumbnailUrl?: string; isPublic?: boolean; dateTime?: string }
type CompactResponse = { images?: CompactImage[]; currentImages?: number; maxImages?: number }

type ImageMetadata = {
  userName?: string
  userAddress?: string
  realm?: string
  scene?: { name?: string; location?: { x?: string | number; y?: string | number } }
  visiblePeople?: Array<{ userName?: string; userAddress?: string; isGuest?: boolean }>
}
type MetadataResponse = { metadata?: ImageMetadata }

const num = (v: string | number | undefined): number | undefined => {
  if (v == null) return undefined
  const n = typeof v === 'number' ? v : Number(v)
  return Number.isFinite(n) ? n : undefined
}

async function fetchGallery(ctx: Ctx, address: string): Promise<void> {
  const r = await signed<CompactResponse>(
    `${await base()}/users/${address}/images?limit=100&offset=0&compact=true`
  ).catch(() => undefined)
  const images = r?.images ?? []
  const photos: GalleryPhoto[] = images
    .filter((i) => typeof i.url === 'string')
    .map((i) => ({
      id: i.id,
      url: i.url as string,
      thumbnailUrl: i.thumbnailUrl,
      dateTime: i.dateTime ?? '',
      isPublic: i.isPublic
    }))
  ctx.send({ kind: 'gallery', photos, current: r?.currentImages ?? photos.length, max: r?.maxImages ?? 0 })
}

export function registerGallery(ctx: Ctx): void {
  ctx.on('getGallery', async () => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'gallery', photos: [], current: 0, max: 0 })
      return
    }
    await fetchGallery(ctx, player.userId)
  })

  ctx.on('getGalleryPhoto', async (msg) => {
    const r = await signed<MetadataResponse>(`${await base()}/images/${msg.id}/metadata`).catch(() => undefined)
    const m = r?.metadata
    if (m == null) {
      ctx.send({ kind: 'galleryPhoto', id: msg.id, meta: null })
      return
    }
    const meta: GalleryPhotoMeta = {
      userName: m.userName,
      userAddress: m.userAddress,
      sceneName: m.scene?.name,
      x: num(m.scene?.location?.x),
      y: num(m.scene?.location?.y),
      realm: m.realm,
      people: (m.visiblePeople ?? [])
        .filter((p) => typeof p.userAddress === 'string')
        .map((p) => ({ address: p.userAddress as string, name: p.userName ?? '', isGuest: p.isGuest }))
    }
    ctx.send({ kind: 'galleryPhoto', id: msg.id, meta })
  })

  ctx.on('deleteGalleryPhoto', async (msg) => {
    await signed(`${await base()}/images/${msg.id}`, 'DELETE').catch((e: unknown) => {
      console.error('[gallery] delete failed', e)
    })
    const player = getPlayer()
    if (player != null) await fetchGallery(ctx, player.userId) // re-emit the updated gallery
  })
}
