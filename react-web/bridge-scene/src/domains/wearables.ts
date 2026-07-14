// Wearables / backpack: owned + equipped wearables, and equipping.
//   from: catalyst GET /explorer/:address/wearables (catalog), @dcl/sdk getPlayer().wearables
//         (equipped), BevyApi.setAvatar (equip).
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import { catalystBase, getJson } from '../http'
import type { Ctx } from '../bridge'
import type { Wearable } from '../../../src/engine/protocol'

type CatalogElement = {
  urn: string
  name: string
  rarity: string
  category: string
  amount?: number
  // Per-owned-token data; carries the tokenId we need for the deployable URN.
  individualData?: Array<{ id?: string; tokenId?: string }>
  entity?: { metadata?: { thumbnail?: string }; content?: Array<{ file: string; hash: string }> }
}

async function fetchCatalog(address: string): Promise<{ baseUrl: string; elements: CatalogElement[] }> {
  const baseUrl = await catalystBase()
  const url = `${baseUrl}/explorer/${address}/wearables?pageNum=1&pageSize=200&includeEntities=true`
  const data = await getJson<{ elements?: CatalogElement[] }>(url).catch(() => undefined)
  return { baseUrl, elements: data?.elements ?? [] }
}

// The owned-wearables catalog rarely changes within a session → cache it (and the token index it
// feeds) per address, so re-opening the Backpack doesn't refetch. Equipped state is read live from
// getPlayer().wearables on each getWearables, so it stays current without invalidating the cache.
let catalogCache: { address: string; baseUrl: string; elements: CatalogElement[] } | null = null
async function getCatalog(address: string): Promise<{ baseUrl: string; elements: CatalogElement[] }> {
  if (catalogCache?.address === address) return catalogCache
  const { baseUrl, elements } = await fetchCatalog(address)
  indexTokens(elements)
  catalogCache = { address, baseUrl, elements }
  return catalogCache
}

// Collection wearables must be referenced in the DEPLOYED profile by their full token URN
// (…:{contract}:{itemId}:{tokenId}); the catalyst rejects the bare item URN with
// "should be an item, not an asset. The URN must include the tokenId.". The catalog's
// individualData carries the owned tokenId per item, so we map item-urn → token-urn and send
// the token form on equip. Base (off-chain) wearables have no tokenId and pass through unchanged.
const tokenUrnByItem = new Map<string, string>()

function tokenUrnFor(el: CatalogElement): string {
  const d = el.individualData?.[0]
  if (d?.id != null && d.id.startsWith(`${el.urn}:`)) return d.id
  if (d?.tokenId != null && d.tokenId !== '') return `${el.urn}:${d.tokenId}`
  return el.urn
}

function indexTokens(elements: CatalogElement[]): void {
  tokenUrnByItem.clear()
  for (const el of elements) {
    const full = tokenUrnFor(el)
    if (full !== el.urn) tokenUrnByItem.set(el.urn, full)
  }
}

// Reverse of tokenUrnFor: a deployed/owned urn may carry a tokenId
// (…:collections-v2:<contract>:<itemId>:<tokenId>); the item form drops it. Base urns pass through.
function itemUrnOf(urn: string): string {
  const parts = urn.split(':')
  if (parts[3] === 'collections-v2' && parts.length > 6) return parts.slice(0, 6).join(':')
  return urn
}

type WearableDef = { id: string; name?: string; rarity?: string; thumbnail?: string; data?: { category?: string } }

// Resolve wearable definitions by item urn (for equipped items absent from the catalog page), to
// learn each one's category (slot placement). Batched to keep the URL length bounded; failures skip.
async function resolveByUrn(baseUrl: string, itemUrns: string[]): Promise<Map<string, WearableDef>> {
  const out = new Map<string, WearableDef>()
  const CHUNK = 50
  for (let i = 0; i < itemUrns.length; i += CHUNK) {
    const qs = itemUrns.slice(i, i + CHUNK).map((u) => `wearableId=${u}`).join('&')
    const data = await getJson<{ wearables?: WearableDef[] }>(`${baseUrl}/lambdas/collections/wearables?${qs}`).catch(() => undefined)
    for (const w of data?.wearables ?? []) out.set(w.id, w)
  }
  return out
}

export function registerWearables(ctx: Ctx): void {
  ctx.on('equip', async (msg) => {
    const me = getPlayer()
    // Ensure the item→token map is loaded (equipping before the catalog was fetched).
    if (me != null && tokenUrnByItem.size === 0) {
      await getCatalog(me.userId)
    }
    const wearableUrns = msg.urns.map((u) => tokenUrnByItem.get(u) ?? u)
    BevyApi.setAvatar({
      equip: { wearableUrns, emoteUrns: (me?.emotes ?? []).map(String), forceRender: [] }
    }).catch((e: unknown) => {
      console.error('[wearables] equip failed', e)
    })
  })

  ctx.on('getWearables', async () => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'wearables', wearables: [], equipped: [] })
      return
    }
    const { baseUrl, elements } = await getCatalog(player.userId)
    const owned = (player.wearables ?? []).map(String)
    const wearables: Wearable[] = elements.map((el) => {
      const file = el.entity?.metadata?.thumbnail
      const hash = el.entity?.content?.find((c) => c.file === file)?.hash
      return {
        urn: el.urn,
        name: el.name,
        rarity: el.rarity,
        category: el.category,
        thumbnail: hash != null ? `${baseUrl}/content/contents/${hash}` : undefined,
        count: el.amount,
        equipped: owned.some((w) => w === el.urn || w.startsWith(`${el.urn}:`))
      }
    })

    // Equipped slots are DECOUPLED from the (paginated) catalog: take the avatar's equipped urns,
    // reuse the catalog entry when present, else batch-resolve the item's category by urn. Each
    // slot's thumbnail is a pure function of the urn — so every equipped item resolves regardless
    // of which catalog page it falls on. Mirrors unity-explorer / bevy-ui-scene.
    const byItemUrn = new Map<string, Wearable>()
    for (const w of wearables) byItemUrn.set(w.urn, w)
    const equippedItemUrns = [...new Set(owned.map(itemUrnOf))]
    const missing = equippedItemUrns.filter((u) => !byItemUrn.has(u))
    const resolved = missing.length > 0 ? await resolveByUrn(baseUrl, missing) : new Map<string, WearableDef>()
    const equipped: Wearable[] = equippedItemUrns
      .map((itemUrn): Wearable | null => {
        const fromCatalog = byItemUrn.get(itemUrn)
        if (fromCatalog != null) return { ...fromCatalog, equipped: true }
        const def = resolved.get(itemUrn)
        const category = def?.data?.category
        if (category == null) return null // can't place a slot without its category
        return {
          urn: itemUrn,
          name: def?.name ?? '',
          rarity: def?.rarity ?? 'base',
          category,
          thumbnail: `${baseUrl}/lambdas/collections/contents/${itemUrn}/thumbnail`,
          equipped: true
        }
      })
      .filter((w): w is Wearable => w != null)

    ctx.send({ kind: 'wearables', wearables, equipped })
  })
}
