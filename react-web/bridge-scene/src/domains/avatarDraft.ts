// The Backpack's unsaved look. Equips, emote slots and outfits edit this draft while the Backpack is
// open, the preview avatar wears it, and closing the Backpack deploys it in one setAvatar (Unity
// publishes the profile when its Backpack closes: BackpackController.Deactivate).
//   from: equip / equipEmote / equipOutfit (editLook), commitAvatar (the page, on close or Retry),
//         revertAvatar (the page, Revert), /lambdas/collections (which body shapes each item fits,
//         via ./collections)
//   to:   BevyApi.setAvatar; avatarSaveFailed to the page
import { getPlayer } from '@dcl/sdk/players'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'
import { catalystBase } from '../http'
import { resolveDefsByUrn } from './collections'
import { itemUrn } from './urns'
import { lookDeploy, sameLook, type AvatarLook } from '../../../src/engine/avatarEquip'
import { bodyShapesOf, isCompatible } from '../../../src/engine/bodyShape'

let draft: AvatarLook | null = null
// The look last known to be on the server: the draft's seed, then each successful deploy. A failed
// deploy leaves it behind so the next close retries — the engine keeps the change locally but won't
// redeploy it by itself.
let deployed: AvatarLook | null = null

function playerLook(): AvatarLook | null {
  const p = getPlayer()
  if (p == null) return null
  return {
    bodyShape: p.avatar?.bodyShapeUrn ?? '',
    eyes: p.avatar?.eyesColor ?? null,
    hair: p.avatar?.hairColor ?? null,
    skin: p.avatar?.skinColor ?? null,
    wearables: (p.wearables ?? []).map(String),
    emotes: (p.emotes ?? []).map(String),
    forceRender: (p.forceRender ?? []).map(String)
  }
}

/** What the Backpack shows: the draft, else the live player. */
export function currentLook(): AvatarLook | null {
  return draft ?? playerLook()
}

/** Change the draft (seeded from the live player on the first edit). Nothing deploys until commit. */
export function editLook(change: Partial<AvatarLook>): void {
  const from = currentLook()
  if (from == null) return
  if (draft == null) deployed = from
  draft = { ...from, ...change }
}

// The look as it deploys: without the wearables that can't render on its body shape (Unity unequips
// them when the body shape changes). The draft keeps them, so switching the body shape and back while
// the Backpack is open loses nothing. An item the catalyst can't resolve is kept.
async function fittingLook(look: AvatarLook): Promise<AvatarLook> {
  const defs = await resolveDefsByUrn('wearables', await catalystBase(), look.wearables.map(itemUrn))
  const fits = (urn: string): boolean => {
    const def = defs.get(itemUrn(urn))
    if (def == null) return true
    return isCompatible({ category: def.data?.category ?? '', bodyShapes: bodyShapesOf(def.data?.representations) }, look.bodyShape || undefined)
  }
  return { ...look, wearables: look.wearables.filter(fits) }
}

// sendEquipped (./wearables) is passed in: it reads this module, so importing it here would be a cycle.
export function registerAvatarDraft(ctx: Ctx, sendEquipped: () => Promise<void>): void {
  ctx.on('commitAvatar', async () => {
    const edited = draft
    const from = deployed
    const me = getPlayer()
    if (edited == null || from == null || me == null) return
    const look = await fittingLook(edited)
    // The page keeps its own copy of the equipped set, which still has the dropped wearables.
    const dropped = look.wearables.length !== edited.wearables.length
    if (sameLook(look, from)) {
      if (draft === edited) draft = null
      if (dropped) await sendEquipped()
      return
    }
    try {
      await BevyApi.setAvatar(lookDeploy(look, from, me.name))
    } catch (e) {
      // "cancelled" isn't a failure: a later setAvatar (another close, a profile save) replaced this
      // one before it deployed, and the engine's copy that one deploys already carries this look.
      if (!String(e).includes('cancelled')) {
        console.error('[avatar] deploy failed', e)
        ctx.send({ kind: 'avatarSaveFailed', message: String(e) })
        return
      }
    }
    deployed = look
    if (draft === edited) draft = null
    if (dropped) await sendEquipped()
  })

  // Give up on a look the server keeps rejecting (a bad item, say) and put the last deployed one back.
  // The server still has that one; redeploying it resets the engine's copy, which holds the rejected
  // look and would carry it into every later save. It shows straight away, before the deploy lands.
  ctx.on('revertAvatar', async () => {
    const rejected = draft
    const from = deployed
    const me = getPlayer()
    if (rejected == null || from == null || me == null) return
    draft = from
    await BevyApi.setAvatar(lookDeploy(from, rejected, me.name)).catch((e: unknown) => {
      console.error('[avatar] revert failed', e)
    })
    if (draft === from) draft = null
  })
}
