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

// A deployed/owned urn may carry a tokenId (…:collections-v{1,2}:<contract>:<itemId>:<tokenId>);
// the item form drops it. Both v1 (ethereum) and v2 (matic) items are 6 segments, so a trailing
// token makes it >6 — strip it for either, else the equipped-set resolve (which needs the bare
// item urn) misses the item and its category slot renders empty. Base urns pass through.
function itemUrnOf(urn: string): string {
  const parts = urn.split(':')
  if ((parts[3] === 'collections-v2' || parts[3] === 'collections-v1') && parts.length > 6) {
    return parts.slice(0, 6).join(':')
  }
  return urn
}

type WearableDef = { id: string; name?: string; rarity?: string; thumbnail?: string; collectionAddress?: string; data?: { category?: string } }

// --- marketplace shop deep links ------------------------------------------------
// The shop link is /shop/item/<contract>/<numeric item id>. A collections-v2 (matic) urn already
// ends in that item id, so it's built for free. Legacy collections-v1 (ethereum) items are
// slug-identified instead (…:mf_sammichgamer:mf_animehair) and the catalyst never carries their
// numeric id, so those are looked up in the marketplace items API — the same API bevy-ui-scene
// uses for item data (see its promise-utils fetchWearable). Its `id` is `<contract>-<last urn
// segment>` for BOTH versions, and repeated `id=` params batch, so one request resolves every
// legacy item at once.
const SHOP_ITEM_BASE = 'https://decentraland.org/shop/item'
const MARKETPLACE_ITEMS = 'https://marketplace-api.decentraland.org/v1/items'

// Resolved links are cached for the session: an item's contract + item id are immutable on-chain.
const shopUrlByItemUrn = new Map<string, string>()

// The contract: from the resolved definition, or straight out of a collections-v2 urn (which
// embeds it) when the definition is missing.
function contractOf(itemUrn: string, collectionAddress?: string): string | undefined {
  if (collectionAddress != null && collectionAddress.startsWith('0x')) return collectionAddress
  const parts = itemUrn.split(':')
  return parts[4] != null && parts[4].startsWith('0x') ? parts[4] : undefined
}

type MarketplaceItem = { urn?: string; itemId?: string; contractAddress?: string }

/** Shop links for the given items, keyed by item urn. Items with no on-chain listing (base /
 *  off-chain wearables and emotes, or a lookup failure) are simply absent from the map — callers
 *  render no SHOP action for those rather than a dead link. */
export async function resolveShopUrls(items: Array<{ urn: string; collectionAddress?: string }>): Promise<Map<string, string>> {
  const out = new Map<string, string>()
  const pendingIds: string[] = []
  for (const { urn, collectionAddress } of items) {
    const cached = shopUrlByItemUrn.get(urn)
    if (cached != null) {
      out.set(urn, cached)
      continue
    }
    const contract = contractOf(urn, collectionAddress)
    const last = urn.split(':').pop()
    if (contract == null || last == null || last === '') continue
    if (/^\d+$/.test(last)) {
      const url = `${SHOP_ITEM_BASE}/${contract}/${last}`
      shopUrlByItemUrn.set(urn, url)
      out.set(urn, url)
      continue
    }
    pendingIds.push(`${contract}-${last}`)
  }
  const CHUNK = 25 // bounds URL length, like the catalyst resolves above
  for (let i = 0; i < pendingIds.length; i += CHUNK) {
    const qs = pendingIds.slice(i, i + CHUNK).map((id) => `id=${id}`).join('&')
    const data = await getJson<{ data?: MarketplaceItem[] }>(`${MARKETPLACE_ITEMS}?${qs}`).catch((e: unknown) => {
      // Non-fatal: those items just show no SHOP button (and aren't cached, so a later open retries).
      console.error('[wearables] marketplace item-id lookup failed', e)
      return undefined
    })
    for (const it of data?.data ?? []) {
      if (it.urn == null || it.contractAddress == null || it.itemId == null) continue
      const url = `${SHOP_ITEM_BASE}/${it.contractAddress}/${it.itemId}`
      shopUrlByItemUrn.set(it.urn, url)
      out.set(it.urn, url)
    }
  }
  return out
}

// A wearable's DEFINITION (name/category/thumbnail/model/rarity) is stable enough within a session to
// cache, though NOT truly immutable: a creator can re-publish edits (new content entity) under the
// same urn. The cache is in-memory and session-lifetime, so at worst it serves stale metadata until a
// reload — acceptable for the HUD. Cache defs by ITEM urn (tokenId already stripped by itemUrnOf;
// every token of an item shares one entry) to serve repeat resolves — getWearables on reopen,
// equipOutfit — from memory instead of re-hitting the catalyst. Mirrors bevy-ui-scene's
// catalystMetadataMap. Misses aren't cached (a transient failure or a not-yet-resolvable urn is
// retried next time).
const defByItemUrn = new Map<string, WearableDef>()

// Resolve wearable definitions by item urn (equipped set) to learn each one's category (slot
// placement). Cached hits skip the network; only misses are fetched, batched to bound URL length.
async function resolveByUrn(baseUrl: string, itemUrns: string[]): Promise<Map<string, WearableDef>> {
  const out = new Map<string, WearableDef>()
  const missing: string[] = []
  for (const u of itemUrns) {
    const cached = defByItemUrn.get(u)
    if (cached != null) out.set(u, cached)
    else missing.push(u)
  }
  const CHUNK = 50
  for (let i = 0; i < missing.length; i += CHUNK) {
    const qs = missing.slice(i, i + CHUNK).map((u) => `wearableId=${u}`).join('&')
    const data = await getJson<{ wearables?: WearableDef[] }>(`${baseUrl}/lambdas/collections/wearables?${qs}`).catch((e: unknown) => {
      // Loud on purpose: a swallowed chunk failure shrinks the resolved set, and downstream that
      // once meant silently undressing the avatar on the next equip (see resolveEquippedSet).
      console.error('[wearables] resolve-by-urn chunk failed', e)
      return undefined
    })
    for (const w of data?.wearables ?? []) {
      defByItemUrn.set(w.id, w)
      out.set(w.id, w)
    }
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

// Resolve a set of (possibly token-form) urns into the equipped category-slot list, indexing
// item→token so a later equip can deploy them. Resolution is by urn (catalyst lambdas) and DECOUPLED
// from the paged grid, so every item resolves regardless of which catalog page is loaded — shared by
// `getWearables` (the live avatar) and `equipOutfit` (a saved outfit's wearables). Mirrors
// bevy-ui-scene's fetchWearablesData(...)(...wearables) on outfit equip.
export async function resolveEquippedSet(urns: string[]): Promise<Wearable[]> {
  const baseUrl = await catalystBase()
  // Equipped urns are the deployable token form → index item→token now (equip needs it even
  // before any grid page is fetched).
  for (const u of urns) {
    const item = itemUrnOf(u)
    if (item !== u) tokenUrnByItem.set(item, u)
  }
  const equippedItemUrns = [...new Set(urns.map(itemUrnOf))]
  const resolved = equippedItemUrns.length > 0 ? await resolveByUrn(baseUrl, equippedItemUrns) : new Map<string, WearableDef>()
  const shopUrls = await resolveShopUrls(
    equippedItemUrns.map((u) => ({ urn: u, collectionAddress: resolved.get(u)?.collectionAddress }))
  )
  return equippedItemUrns.map((itemUrn): Wearable => {
    const def = resolved.get(itemUrn)
    return {
      urn: itemUrn,
      name: def?.name ?? '',
      rarity: def?.rarity ?? 'base',
      // No def (resolve failure or an urn the lambda doesn't know): keep the item under 'unknown'
      // rather than dropping it — 'unknown' renders in no category slot, but the item stays in the
      // equipped set, so the next equip round-trip (equipSetWith) still deploys it. Dropping here
      // silently undressed the avatar whenever the lambdas request failed mid-session.
      category: def?.data?.category ?? 'unknown',
      thumbnail: `${baseUrl}/lambdas/collections/contents/${itemUrn}/thumbnail`,
      equipped: true,
      shopUrl: shopUrls.get(itemUrn)
    }
  })
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

  // Equipped set (category slots) for the live avatar, resolved by urn — DECOUPLED from the paged
  // grid so every equipped item shows regardless of which catalog page it's on.
  ctx.on('getWearables', async () => {
    const player = getPlayer()
    if (player == null) {
      ctx.send({ kind: 'wearables', equipped: [] })
      return
    }
    ctx.send({ kind: 'wearables', equipped: await resolveEquippedSet((player.wearables ?? []).map(String)) })
  })
}
