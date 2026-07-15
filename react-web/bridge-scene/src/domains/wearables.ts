// Wearables / backpack: equipped wearables (category slots) + equipping, plus the paged owned
// catalog fetcher used by the generic `catalog` domain.
//   from: catalyst GET /explorer/:address/wearables (owned catalog, paged),
//         GET /lambdas/collections/wearables?wearableId=… (equipped-by-urn resolve),
//         @dcl/sdk getPlayer().wearables (equipped), BevyApi.setAvatar (equip).
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

// Collection wearables must be referenced in the DEPLOYED profile by their full token URN
// (…:{contract}:{itemId}:{tokenId}); the catalyst rejects the bare item URN with
// "should be an item, not an asset. The URN must include the tokenId.". individualData carries the
// owned tokenId per item, so we map item-urn → token-urn and send the token form on equip. Base
// (off-chain) wearables have no tokenId and pass through unchanged. The map accumulates across
// fetched pages + the equipped set, so any item the user has actually seen/equipped can be equipped.
const tokenUrnByItem = new Map<string, string>()

function tokenUrnFor(el: CatalogElement): string {
  const d = el.individualData?.[0]
  if (d?.id != null && d.id.startsWith(`${el.urn}:`)) return d.id
  if (d?.tokenId != null && d.tokenId !== '') return `${el.urn}:${d.tokenId}`
  return el.urn
}

function accumulateTokens(elements: CatalogElement[]): void {
  for (const el of elements) {
    const full = tokenUrnFor(el)
    if (full !== el.urn) tokenUrnByItem.set(el.urn, full)
  }
}

// A deployed/owned urn may carry a tokenId (…:collections-v2:<contract>:<itemId>:<tokenId>); the
// item form drops it. Base urns pass through.
function itemUrnOf(urn: string): string {
  const parts = urn.split(':')
  if (parts[3] === 'collections-v2' && parts.length > 6) return parts.slice(0, 6).join(':')
  return urn
}

type WearableDef = { id: string; name?: string; rarity?: string; thumbnail?: string; data?: { category?: string } }

// Resolve wearable definitions by item urn (equipped set) to learn each one's category (slot
// placement). Batched to keep the URL length bounded; failures skip.
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

export interface CatalogPageParams {
  /** 0-based page. */
  page: number
  pageSize: number
  category?: string
  search?: string
  orderBy?: 'rarity' | 'name'
  direction?: 'asc' | 'desc'
  collectiblesOnly?: boolean
}

// Server-side-paginated owned-wearables fetch (one page). Filters/sort are applied by the catalyst
// so multi-thousand inventories never load at once. `equipped` per item reflects the live avatar.
export async function fetchWearablesPage(address: string, p: CatalogPageParams): Promise<{ items: Wearable[]; total: number }> {
  const baseUrl = await catalystBase()
  let url = `${baseUrl}/explorer/${address}/wearables?pageNum=${p.page + 1}&pageSize=${p.pageSize}&includeEntities=true`
  if (p.category != null && p.category !== 'all') url += `&category=${p.category}`
  if (p.search != null && p.search !== '') url += `&name=${encodeURIComponent(p.search)}`
  if (p.orderBy != null) url += `&orderBy=${p.orderBy}&direction=${p.direction === 'asc' ? 'ASC' : 'DESC'}`
  // Explicit collection types (matches unity/bevy-ui-scene): collectibles-only drops base wearables.
  const collectionTypes = p.collectiblesOnly ? ['on-chain', 'third-party'] : ['base-wearable', 'on-chain', 'third-party']
  for (const t of collectionTypes) url += `&collectionType=${t}`

  const data = await getJson<{ elements?: CatalogElement[]; totalAmount?: number }>(url).catch(() => undefined)
  const elements = data?.elements ?? []
  accumulateTokens(elements)
  const owned = (getPlayer()?.wearables ?? []).map(String)
  const items: Wearable[] = elements.map((el) => {
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
  return { items, total: data?.totalAmount ?? items.length }
}

export function registerWearables(ctx: Ctx): void {
  ctx.on('equip', (msg) => {
    const me = getPlayer()
    const wearableUrns = msg.urns.map((u) => tokenUrnByItem.get(u) ?? u)
    BevyApi.setAvatar({
      equip: { wearableUrns, emoteUrns: (me?.emotes ?? []).map(String), forceRender: [] }
    }).catch((e: unknown) => {
      console.error('[wearables] equip failed', e)
    })
  })

  // Equipped set (category slots), resolved by urn — DECOUPLED from the paged grid so every equipped
  // item shows regardless of which catalog page it's on. Mirrors unity-explorer / bevy-ui-scene.
  ctx.on('getWearables', async () => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'wearables', equipped: [] })
      return
    }
    const baseUrl = await catalystBase()
    const owned = (player.wearables ?? []).map(String)
    // Equipped urns are the deployable token form → index item→token now (equip needs it even
    // before any grid page is fetched).
    for (const u of owned) {
      const item = itemUrnOf(u)
      if (item !== u) tokenUrnByItem.set(item, u)
    }
    const equippedItemUrns = [...new Set(owned.map(itemUrnOf))]
    const resolved = equippedItemUrns.length > 0 ? await resolveByUrn(baseUrl, equippedItemUrns) : new Map<string, WearableDef>()
    const equipped: Wearable[] = equippedItemUrns
      .map((itemUrn): Wearable | null => {
        const category = resolved.get(itemUrn)?.data?.category
        if (category == null) return null // can't place a slot without its category
        const def = resolved.get(itemUrn)
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

    ctx.send({ kind: 'wearables', equipped })
  })
}
