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

export function registerWearables(ctx: Ctx): void {
  ctx.on('equip', async (msg) => {
    const me = getPlayer()
    // Ensure the item→token map is loaded (equipping before the catalog was fetched).
    if (me != null && tokenUrnByItem.size === 0) {
      indexTokens((await fetchCatalog(me.userId)).elements)
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
      ctx.send({ kind: 'wearables', wearables: [] })
      return
    }
    const { baseUrl, elements } = await fetchCatalog(player.userId)
    indexTokens(elements)
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
    ctx.send({ kind: 'wearables', wearables })
  })
}
