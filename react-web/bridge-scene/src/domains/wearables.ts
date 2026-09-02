// Wearables / backpack: equipped wearables (category slots) + equipping, plus the paged owned
// catalog fetcher used by the generic `catalog` domain.
//   from: catalyst GET /explorer/:address/wearables (owned catalog, paged),
//         GET /lambdas/collections/wearables (equipped-by-urn resolve, via ./collections),
//         @dcl/sdk getPlayer().wearables (equipped), BevyApi.setAvatar (equip).
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import { catalystBase, getJson } from '../http'
import { resolveDefsByUrn, thumbnailUrl } from './collections'
import { resolveShopUrls } from './marketplace'
import { itemUrn, tokenUrnOf } from './urns'
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

// item-urn → deployable token urn (see tokenUrnOf), what the equip handler sends. The map
// accumulates across fetched pages + the equipped set, so any item the user has actually
// seen/equipped can be equipped.
const tokenUrnByItem = new Map<string, string>()

function accumulateTokens(elements: CatalogElement[]): void {
  for (const el of elements) {
    const full = tokenUrnOf(el)
    if (full !== el.urn) tokenUrnByItem.set(el.urn, full)
  }
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

type ResolveOpts = {
  /** Opt-in: only the passport's EquippedItemCard renders a SHOP action — the Backpack's
   *  WearableCard ignores it — and resolving a legacy (collections-v1) item costs a marketplace-api
   *  round trip the Backpack and outfit equip would pay for nothing. */
  shopUrls?: boolean
}

// Resolve a set of (possibly token-form) urns into displayable wearables. Resolution is by urn
// (catalyst lambdas) and DECOUPLED from the paged grid, so every item resolves regardless of which
// catalog page is loaded. Pure — no side effects — so it serves ANY address's urns (another user's
// passport). Mirrors bevy-ui-scene's fetchWearablesData(...)(...wearables) on outfit equip.
export async function resolveWearables(urns: string[], opts: ResolveOpts = {}): Promise<Wearable[]> {
  const baseUrl = await catalystBase()
  const equippedItemUrns = [...new Set(urns.map(itemUrn))]
  const resolved = await resolveDefsByUrn('wearables', baseUrl, equippedItemUrns)
  const shopUrls =
    opts.shopUrls === true
      ? await resolveShopUrls(equippedItemUrns.map((u) => ({ urn: u, collectionAddress: resolved.get(u)?.collectionAddress })))
      : undefined
  return equippedItemUrns.map((item): Wearable => {
    const def = resolved.get(item)
    return {
      urn: item,
      name: def?.name ?? '',
      rarity: def?.rarity ?? 'base',
      // No def (resolve failure or an urn the lambda doesn't know): keep the item under 'unknown'
      // rather than dropping it — 'unknown' renders in no category slot, but the item stays in the
      // equipped set, so the next equip round-trip (equipSetWith) still deploys it. Dropping here
      // silently undressed the avatar whenever the lambdas request failed mid-session.
      category: def?.data?.category ?? 'unknown',
      thumbnail: thumbnailUrl(baseUrl, item),
      equipped: true,
      shopUrl: shopUrls?.get(item)
    }
  })
}

// The LOCAL PLAYER's equipped set: resolveWearables plus item→token indexing so a later equip can
// deploy them (equip needs it even before any grid page is fetched). Shared by `getWearables` (the
// live avatar), `equipOutfit` (a saved outfit's wearables) and the passport's own-profile path.
// Own urns ONLY: tokenUrnByItem is keyed by ITEM urn and is what the equip handler deploys, so
// another user's tokenId would overwrite ours for a commonly-owned item and the next equip would
// claim a token we don't own — the catalyst rejects that deploy and the change silently doesn't
// persist. Anyone else's urns go through resolveWearables.
export async function resolveEquippedSet(urns: string[], opts: ResolveOpts = {}): Promise<Wearable[]> {
  for (const u of urns) {
    const item = itemUrn(u)
    if (item !== u) tokenUrnByItem.set(item, u)
  }
  return await resolveWearables(urns, opts)
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
