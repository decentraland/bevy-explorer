// Outfits: the player's saved avatar looks (backpack Outfits tab).
//   load  ← localStorage cache first, else catalyst GET /lambdas/outfits/:address (unsigned)
//   save/delete → localStorage (Phase 1; Phase 2 will deploy a signed `outfits` entity)
//   equip → BevyApi.setAvatar (body shape + colors + wearables)
//
// Mirrors bevy-ui-scene's outfits flow (localStorage-first read, host-mediated equip). The saved
// shape matches the deployed catalyst `outfits` entity so Phase 2 can deploy it unchanged.
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import { catalystBase, getJson } from '../http'
import { resolveEquippedSet } from './wearables'
import type { Ctx } from '../bridge'
import type { Outfit, OutfitsMetadata, RGBColor } from '../../../src/engine/protocol'

// The Bevy engine injects a persistent localStorage into system scenes (same as bevy-ui-scene).
declare const localStorage: {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
  removeItem(key: string): void
}

const EMPTY: OutfitsMetadata = { outfits: [], namesForExtraSlots: [] }
const GREY: RGBColor = { r: 0.5, g: 0.5, b: 0.5 }

const storageKey = (address: string): string => `outfits:${address.toLowerCase()}`

function readLocal(address: string): OutfitsMetadata | null {
  try {
    const raw = localStorage.getItem(storageKey(address))
    return raw != null ? (JSON.parse(raw) as OutfitsMetadata) : null
  } catch {
    return null
  }
}

function writeLocal(address: string, metadata: OutfitsMetadata): void {
  try {
    localStorage.setItem(storageKey(address), JSON.stringify(metadata))
  } catch (e) {
    console.error('[outfits] localStorage write failed', e)
  }
}

// localStorage-first (like bevy-ui-scene) so a local save survives reload; else the catalyst
// lambdas endpoint, which returns { metadata: OutfitsMetadata } (missing → empty for new users).
async function loadMetadata(address: string): Promise<OutfitsMetadata> {
  const local = readLocal(address)
  if (local != null) return local
  const base = await catalystBase()
  const res = await getJson<{ metadata?: OutfitsMetadata }>(`${base}/lambdas/outfits/${address}`).catch(() => undefined)
  return res?.metadata ?? EMPTY
}

// Capture the player's CURRENT look into a deployable Outfit (colors default to grey if absent).
function currentOutfit(player: NonNullable<ReturnType<typeof getPlayer>>): Outfit {
  const a = player.avatar
  return {
    bodyShape: a?.bodyShapeUrn ?? '',
    eyes: { color: a?.eyesColor ?? GREY },
    hair: { color: a?.hairColor ?? GREY },
    skin: { color: a?.skinColor ?? GREY },
    wearables: (player.wearables ?? []).map(String),
    forceRender: (player.forceRender ?? []).map(String)
  }
}

export function registerOutfits(ctx: Ctx): void {
  const emit = (metadata: OutfitsMetadata): void => ctx.send({ kind: 'outfits', metadata })

  ctx.on('getOutfits', async () => {
    const me = getPlayer()
    if (me == null) {
      emit(EMPTY)
      return
    }
    emit(await loadMetadata(me.userId))
  })

  // Save the current look into `slot`, replacing any existing outfit there, then re-emit.
  ctx.on('saveOutfit', async (msg) => {
    const me = getPlayer()
    if (me == null) return
    const metadata = await loadMetadata(me.userId)
    const outfits = metadata.outfits.filter((o) => o.slot !== msg.slot)
    outfits.push({ slot: msg.slot, outfit: currentOutfit(me) })
    outfits.sort((a, b) => a.slot - b.slot)
    const next = { ...metadata, outfits }
    writeLocal(me.userId, next)
    emit(next)
  })

  ctx.on('deleteOutfit', async (msg) => {
    const me = getPlayer()
    if (me == null) return
    const metadata = await loadMetadata(me.userId)
    const next = { ...metadata, outfits: metadata.outfits.filter((o) => o.slot !== msg.slot) }
    writeLocal(me.userId, next)
    emit(next)
  })

  // Apply a saved outfit's body shape, colors and wearables to the live profile (keeps emotes).
  ctx.on('equipOutfit', async (msg) => {
    const me = getPlayer()
    if (me == null) return
    const metadata = await loadMetadata(me.userId)
    const found = metadata.outfits.find((o) => o.slot === msg.slot)
    if (found == null) return
    const { outfit } = found
    BevyApi.setAvatar({
      base: {
        name: me.name,
        bodyShapeUrn: outfit.bodyShape,
        eyesColor: outfit.eyes.color,
        hairColor: outfit.hair.color,
        skinColor: outfit.skin.color
      },
      equip: {
        wearableUrns: outfit.wearables,
        emoteUrns: (me.emotes ?? []).map(String),
        forceRender: outfit.forceRender
      }
    }).catch((e: unknown) => {
      console.error('[outfits] equip failed', e)
    })
    // setAvatar's deploy never pushes a wearables update, so re-emit the equipped set resolved from
    // the outfit's wearables (by urn, independent of the loaded catalog page). Otherwise off-page
    // outfit items never reach the HUD's category slots and the next single-item equip drops them.
    ctx.send({ kind: 'wearables', equipped: await resolveEquippedSet(outfit.wearables.map(String)) })
  })
}
