// Client for the public marketplace catalog (marketplace-api.<base domain>/v1/catalog). Buying is a
// wallet transaction, so purchases finish on the web marketplace (as in Unity).

import { BASE_DOMAIN } from '../../lib/baseDomain'

const CATALOG_API = `https://marketplace-api.${BASE_DOMAIN}/v1/catalog`
const MARKETPLACE = `https://${BASE_DOMAIN}/marketplace`
export const SHOP_PAGE_SIZE = 24

export type ShopCategory = 'wearable' | 'emote'
export type ShopSort = 'recently_listed' | 'recently_sold' | 'cheapest' | 'most_expensive'

export interface ShopItem {
  id: string
  name: string
  thumbnail: string
  rarity: string
  category: string
  /** Marketplace path, e.g. /contracts/0x…/items/1 */
  url: string
  /** Primary sale open (minted from the collection at `price`). */
  isOnSale: boolean
  price: string
  minListingPrice?: string | null
  available?: number
}

export async function fetchShop(
  q: { category: ShopCategory; sortBy: ShopSort; search: string; skip: number },
  signal?: AbortSignal
): Promise<{ items: ShopItem[]; total: number }> {
  const params = new URLSearchParams({ first: String(SHOP_PAGE_SIZE), skip: String(q.skip), category: q.category, sortBy: q.sortBy, isOnSale: 'true' })
  if (q.search.trim() !== '') params.set('search', q.search.trim())
  const res = await fetch(`${CATALOG_API}?${params}`, { signal })
  if (!res.ok) throw new Error(`Marketplace returned ${res.status}`)
  const body = (await res.json()) as { data?: ShopItem[]; total?: number }
  if (!Array.isArray(body.data)) throw new Error('Marketplace returned an unexpected response')
  return { items: body.data, total: body.total ?? body.data.length }
}

/** Wei (18 decimals) → MANA with up to 2 decimals. */
export function formatMana(wei: string): string {
  const mana = Number(BigInt(wei) / 10n ** 16n) / 100
  return String(mana)
}

export function shopItemPrice(item: ShopItem): string {
  const wei = item.isOnSale ? item.price : item.minListingPrice
  if (wei == null) return 'Not for sale'
  return wei === '0' ? 'Free' : `${formatMana(wei)} MANA`
}

export function marketplaceUrl(item: ShopItem): string {
  return `${MARKETPLACE}${item.url}?utm_source=client`
}
