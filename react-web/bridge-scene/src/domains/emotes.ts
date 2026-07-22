// Emotes: the player's OWNED emote collection + which are equipped to the 10 wheel slots, playing
// one, and assigning one to a slot.
//   from: @dcl/sdk getPlayer().emotes (equipped, by slot index), catalyst GET
//         /explorer/:address/emotes (owned catalog), RestrictedActions.triggerEmote (play),
//         BevyApi.setAvatar (assign).
import { getPlayer } from '@dcl/sdk/players'
import { triggerEmote } from '~system/RestrictedActions'
import { BevyApi } from '../bevy-api'
import { catalystBase, getJson } from '../http'
import type { Ctx } from '../bridge'
import type { Emote } from '../../../src/engine/protocol'

const SLOT_COUNT = 10 // the emote wheel has 10 slots
const BASE_EMOTES = ['handsair', 'wave', 'fistpump', 'dance', 'raisehand', 'clap', 'money', 'kiss', 'headexplode', 'shrug'].map(
  (n) => `urn:decentraland:off-chain:base-emotes:${n}`
)

// Equipped/owned emote urns can carry an NFT token id:
//   urn:decentraland:matic:collections-v2:<contract>:<itemId>[:<tokenId>]
// The catalog keys on the ITEM urn (no tokenId), so strip it for lookups/dedupe.
function itemUrn(urn: string): string {
  const parts = urn.split(':')
  if (parts[3] === 'collections-v2' && parts.length > 6) return parts.slice(0, 6).join(':')
  return urn
}
const isBase = (urn: string): boolean => BASE_EMOTES.includes(itemUrn(urn))

// The 10 wheel slots to show for a profile: its equipped emotes positionally, or — when the wheel is
// entirely empty (a fresh profile) — the base emotes in order (slot i = BASE_EMOTES[i]). The bevy
// runtime doesn't seed the defaults into getPlayer().emotes (bevy-ui-scene hits the same empty array),
// so the HUD fills them here, mirroring Unity's SelfProfile empty-wheel fill: it's all-or-nothing, so
// once any slot is equipped the remaining empties stay empty.
function equippedSlots(emotes: readonly unknown[] | undefined): string[] {
  const slots = Array.from({ length: SLOT_COUNT }, (_, i) => String((emotes ?? [])[i] ?? ''))
  return slots.every((u) => u === '') ? [...BASE_EMOTES] : slots
}

// Title-case a base-emote id ("raisehand" → "Raise Hand"); custom emotes use the catalog name.
function baseEmoteName(urn: string): string {
  const i = urn.indexOf('base-emotes:')
  const seg = i >= 0 ? urn.slice(i + 'base-emotes:'.length) : urn
  return seg.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

type CatalogElement = {
  urn: string
  name?: string
  rarity?: string
  amount?: number
  individualData?: Array<{ id?: string; tokenId?: string }>
  entity?: { metadata?: { name?: string; thumbnail?: string; rarity?: string }; content?: Array<{ file: string; hash: string }> }
}

async function fetchOwned(base: string, address: string): Promise<CatalogElement[]> {
  const url = `${base}/explorer/${address}/emotes?pageNum=1&pageSize=200&includeEntities=true`
  const data = await getJson<{ elements?: CatalogElement[] }>(url).catch(() => undefined)
  return data?.elements ?? []
}

function thumbUrl(base: string, el: CatalogElement): string {
  const file = el.entity?.metadata?.thumbnail
  const hash = el.entity?.content?.find((c) => c.file === file)?.hash
  return hash != null ? `${base}/content/contents/${hash}` : `${base}/lambdas/collections/contents/${el.urn}/thumbnail`
}

// Like wearables: the deployed profile must reference a collection emote by its full token URN
// (…:<contract>:<itemId>:<tokenId>); the catalog's individualData carries the owned tokenId.
const tokenUrnByItem = new Map<string, string>()
function tokenUrnFor(el: CatalogElement): string {
  const d = el.individualData?.[0]
  if (d?.id != null && d.id.startsWith(`${el.urn}:`)) return d.id
  if (d?.tokenId != null && d.tokenId !== '') return `${el.urn}:${d.tokenId}`
  return el.urn
}

// The owned-emotes catalog rarely changes within a session, so cache it (and the item→token map it
// feeds) per address — re-opening the backpack then costs nothing. Equipped SLOTS are NOT cached;
// they're recomputed live from getPlayer().emotes on every getEmotes, so assignments stay current.
let ownedCache: { address: string; elements: CatalogElement[] } | null = null
async function getOwned(base: string, address: string): Promise<CatalogElement[]> {
  if (ownedCache?.address === address) return ownedCache.elements
  const elements = await fetchOwned(base, address)
  ownedCache = { address, elements }
  tokenUrnByItem.clear()
  for (const el of elements) {
    const full = tokenUrnFor(el)
    if (full !== el.urn) tokenUrnByItem.set(el.urn, full)
  }
  return elements
}

// Resolve a slot value for equip: clear → '', base emote → as-is, collection emote → token form.
function equipUrn(urn: string): string {
  if (urn === '') return ''
  if (isBase(urn)) return urn
  return tokenUrnByItem.get(itemUrn(urn)) ?? urn
}

export function registerEmotes(ctx: Ctx): void {
  ctx.on('triggerEmote', (msg) => {
    triggerEmote({ predefinedEmote: msg.urn }).catch((e: unknown) => {
      console.error('[emotes] trigger failed', e)
    })
  })

  // Assign an emote to a wheel slot: rebuild the 10-slot emoteUrns array (preserving the rest) with
  // the chosen emote (token form for collection emotes) at `slot`, then persist via setAvatar.
  ctx.on('equipEmote', async (msg) => {
    const me = getPlayer()
    if (me == null) return
    if (!isBase(msg.urn) && tokenUrnByItem.size === 0) await getOwned(await catalystBase(), me.userId).catch(() => [])
    // Seed from the effective slots (base defaults when the wheel is empty) so equipping into a fresh
    // profile persists the defaults alongside the new one, instead of blanking the other 9 slots.
    const slots = equippedSlots(me.emotes)
    slots[msg.slot] = equipUrn(msg.urn)
    BevyApi.setAvatar({
      equip: { wearableUrns: (me.wearables ?? []).map(String), emoteUrns: slots, forceRender: [] }
    }).catch((e: unknown) => {
      console.error('[emotes] equip failed', e)
    })
  })

  ctx.on('getEmotes', async () => {
    const player = getPlayer()
    const base = await catalystBase()

    // equipped array index = wheel slot; map item-urn → slot (base defaults when the wheel is empty).
    const slotByItem = new Map<string, number>()
    equippedSlots(player?.emotes).forEach((urn, slot) => {
      if (urn !== '') slotByItem.set(itemUrn(urn), slot)
    })

    // base emotes are always available to everyone
    const baseEmotes: Emote[] = BASE_EMOTES.map((urn) => ({
      urn,
      name: baseEmoteName(urn),
      rarity: 'base',
      thumbnail: `${base}/lambdas/collections/contents/${urn}/thumbnail`,
      slot: slotByItem.get(itemUrn(urn))
    }))

    // + owned custom emotes from the catalog (cached per address)
    const owned = player != null ? await getOwned(base, player.userId).catch(() => []) : []
    const customEmotes: Emote[] = owned.map((el) => ({
      urn: el.urn,
      name: el.entity?.metadata?.name ?? el.name ?? baseEmoteName(el.urn),
      rarity: el.rarity ?? el.entity?.metadata?.rarity ?? 'base',
      thumbnail: thumbUrl(base, el),
      count: el.amount,
      slot: slotByItem.get(itemUrn(el.urn))
    }))

    // dedupe by item urn (base first, so a base emote equipped in a slot isn't duplicated)
    const seen = new Set<string>()
    const emotes = [...baseEmotes, ...customEmotes].filter((e) => {
      const k = itemUrn(e.urn)
      if (seen.has(k)) return false
      seen.add(k)
      return true
    })
    ctx.send({ kind: 'emotes', emotes })
  })
}
