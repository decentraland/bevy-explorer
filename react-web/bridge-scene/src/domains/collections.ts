// Catalyst collections lambda: item DEFINITIONS (name / rarity / thumbnail / category / collection
// address) resolved by item urn, for wearables and emotes alike.
//   from: GET /lambdas/collections/wearables?wearableId=… and /lambdas/collections/emotes?emoteId=…
import { getJson } from '../http'

export type ItemDef = {
  id: string
  name?: string
  rarity?: string
  thumbnail?: string
  collectionAddress?: string
  data?: { category?: string }
}

export type ItemKind = 'wearables' | 'emotes'
const QUERY_KEY: Record<ItemKind, string> = { wearables: 'wearableId', emotes: 'emoteId' }

// An item's DEFINITION is stable enough within a session to cache, though NOT truly immutable: a
// creator can re-publish edits (new content entity) under the same urn. The cache is in-memory and
// session-lifetime, so at worst it serves stale metadata until a reload — acceptable for the HUD.
// Cache defs by ITEM urn (tokenId already stripped; every token of an item shares one entry) to serve
// repeat resolves — getWearables on reopen, equipOutfit, passport opens — from memory instead of
// re-hitting the catalyst. Mirrors bevy-ui-scene's catalystMetadataMap. Misses aren't cached (a
// transient failure or a not-yet-resolvable urn is retried next time).
const defByItemUrn: Record<ItemKind, Map<string, ItemDef>> = { wearables: new Map(), emotes: new Map() }

const CHUNK = 50 // bounds URL length

/** Resolve item definitions by item urn. Cached hits skip the network; only misses are fetched,
 *  batched. An empty input costs nothing. */
export async function resolveDefsByUrn(kind: ItemKind, baseUrl: string, itemUrns: string[]): Promise<Map<string, ItemDef>> {
  const cache = defByItemUrn[kind]
  const out = new Map<string, ItemDef>()
  const missing: string[] = []
  for (const u of itemUrns) {
    const cached = cache.get(u)
    if (cached != null) out.set(u, cached)
    else missing.push(u)
  }
  for (let i = 0; i < missing.length; i += CHUNK) {
    const qs = missing.slice(i, i + CHUNK).map((u) => `${QUERY_KEY[kind]}=${u}`).join('&')
    const data = await getJson<Partial<Record<ItemKind, ItemDef[]>>>(`${baseUrl}/lambdas/collections/${kind}?${qs}`).catch((e: unknown) => {
      // Loud on purpose: a swallowed chunk failure shrinks the resolved set, and downstream that
      // once meant silently undressing the avatar on the next equip (see resolveEquippedSet).
      console.error(`[collections] ${kind} resolve-by-urn chunk failed`, e)
      return undefined
    })
    for (const d of data?.[kind] ?? []) {
      cache.set(d.id, d)
      out.set(d.id, d)
    }
  }
  return out
}
