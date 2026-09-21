// Marketplace: shop deep links for on-chain collectibles (wearables and emotes alike).
//   from: marketplace-api GET /v1/items?id=<contract>-<slug> (legacy collections-v1 item ids).
import { getJson } from '../http'

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
      const url = `${SHOP_ITEM_BASE}/${encodeURIComponent(contract)}/${last}`
      shopUrlByItemUrn.set(urn, url)
      out.set(urn, url)
      continue
    }
    pendingIds.push(`${contract}-${last}`)
  }
  const CHUNK = 25 // bounds URL length, like the catalyst resolves in ./collections
  for (let i = 0; i < pendingIds.length; i += CHUNK) {
    const qs = pendingIds.slice(i, i + CHUNK).map((id) => `id=${encodeURIComponent(id)}`).join('&')
    const data = await getJson<{ data?: MarketplaceItem[] }>(`${MARKETPLACE_ITEMS}?${qs}`).catch((e: unknown) => {
      // Non-fatal: those items just show no SHOP button (and aren't cached, so a later open retries).
      console.error('[marketplace] item-id lookup failed', e)
      return undefined
    })
    for (const it of data?.data ?? []) {
      if (it.urn == null || it.contractAddress == null || it.itemId == null) continue
      const url = `${SHOP_ITEM_BASE}/${encodeURIComponent(it.contractAddress)}/${encodeURIComponent(it.itemId)}`
      shopUrlByItemUrn.set(it.urn, url)
      out.set(it.urn, url)
    }
  }
  return out
}
