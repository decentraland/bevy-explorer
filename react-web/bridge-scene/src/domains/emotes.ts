// Emotes: the player's equipped emote wheel + playing one.
//   from: @dcl/sdk getPlayer().emotes, catalyst GET /lambdas/collections/emotes (name + rarity),
//         RestrictedActions.triggerEmote.
import { getPlayer } from '@dcl/sdk/players'
import { triggerEmote } from '~system/RestrictedActions'
import { catalystBase, getJson } from '../http'
import type { Ctx } from '../bridge'
import type { Emote } from '../../../src/engine/protocol'

const BASE_EMOTES = ['handsair', 'wave', 'fistpump', 'dance', 'raisehand', 'clap', 'money', 'kiss', 'headexplode', 'shrug'].map(
  (n) => `urn:decentraland:off-chain:base-emotes:${n}`
)

// Equipped emote urns can carry an NFT token id:
//   urn:decentraland:matic:collections-v2:<contract>:<itemId>[:<tokenId>]
// The catalog endpoint keys on the ITEM urn (no tokenId), so strip it for queries/lookups.
function itemUrn(urn: string): string {
  const parts = urn.split(':')
  // collections-v2 item urn is exactly 6 segments (…:<contract>:<itemId>); drop anything after.
  if (parts[3] === 'collections-v2' && parts.length > 6) return parts.slice(0, 6).join(':')
  return urn
}
const isBase = (urn: string): boolean => BASE_EMOTES.includes(itemUrn(urn))

// Title-case a base-emote id ("raisehand" → "Raise Hand"); custom emotes use the catalog name.
function baseEmoteName(urn: string): string {
  const i = urn.indexOf('base-emotes:')
  const seg = i >= 0 ? urn.slice(i + 'base-emotes:'.length) : urn
  return seg.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

type EmoteCatalogEntry = { id?: string; urn?: string; name?: string; rarity?: string; metadata?: { rarity?: string } }
type EmoteMeta = { name?: string; rarity: string }

// urn (without tokenId) → { name, rarity } for the given custom-emote urns.
async function fetchMeta(base: string, urns: string[]): Promise<Map<string, EmoteMeta>> {
  const out = new Map<string, EmoteMeta>()
  if (urns.length === 0) return out
  const qs = urns.map((u) => `emoteId=${itemUrn(u)}`).join('&')
  const data = await getJson<{ emotes?: EmoteCatalogEntry[]; elements?: EmoteCatalogEntry[] }>(
    `${base}/lambdas/collections/emotes?${qs}`
  ).catch(() => undefined)
  for (const e of data?.emotes ?? data?.elements ?? []) {
    const id = e.id ?? e.urn
    if (id != null) out.set(itemUrn(id), { name: e.name, rarity: e.rarity ?? e.metadata?.rarity ?? 'base' })
  }
  return out
}

export function registerEmotes(ctx: Ctx): void {
  ctx.on('triggerEmote', (msg) => {
    triggerEmote({ predefinedEmote: msg.urn }).catch((e: unknown) => {
      console.error('[emotes] trigger failed', e)
    })
  })

  ctx.on('getEmotes', async () => {
    const equipped = (getPlayer()?.emotes ?? []).filter((e): e is string => typeof e === 'string')
    const urns = equipped.length > 0 ? equipped : BASE_EMOTES
    const base = await catalystBase()
    const metaByUrn = await fetchMeta(base, urns.filter((u) => !isBase(u))).catch(() => new Map<string, EmoteMeta>())

    const emotes: Emote[] = urns.map((urn, slot) => {
      const meta = metaByUrn.get(itemUrn(urn))
      return {
        slot,
        urn,
        name: isBase(urn) ? baseEmoteName(urn) : meta?.name ?? baseEmoteName(urn),
        rarity: isBase(urn) ? 'base' : meta?.rarity ?? 'base',
        thumbnail: `${base}/lambdas/collections/contents/${urn}/thumbnail`
      }
    })
    ctx.send({ kind: 'emotes', emotes })
  })
}
