// Welcome page: where the terms are accepted, and an account without a profile settles the name and
// look its new profile deploys with. The look is the Backpack's draft (./avatarDraft), so the stage
// wears each reroll before anything deploys.
//   from: getWelcome / rerollLook / acceptWelcome (the page), BevyApi.getWelcome, the curated
//         default looks (`default<N>` profiles on the base-domain catalyst, as the engine's)
//   to:   welcome / welcomeAccepted / wearables (the page), BevyApi.setAvatar + acceptTerms,
//         RestrictedActions.openExternalUrl (openLegal)
import { getPlayer } from '@dcl/sdk/players'
import { openExternalUrl } from '~system/RestrictedActions'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'
import { catalystBase, getJson } from '../http'
import type { Color3 } from '../../../src/engine/generated'
import { deployLookAs, editLook, hasDraft } from './avatarDraft'
import { sendEquipped } from './wearables'

// comms::profile::DEFAULT_LOOKS: the looks are deployed as `default1` to `default160`
const DEFAULT_LOOKS = 160

const LEGAL = {
  terms: 'https://decentraland.org/terms/',
  privacy: 'https://decentraland.org/privacy/'
}

type LookColor = { color?: Color3 }
type DefaultLook = {
  avatars?: Array<{
    avatar?: { bodyShape?: string; wearables?: string[]; eyes?: LookColor; hair?: LookColor; skin?: LookColor }
  }>
}

const color = (c: LookColor | undefined): Color3 | null =>
  c?.color == null ? null : { r: c.color.r, g: c.color.g, b: c.color.b }

export function registerWelcome(ctx: Ctx): void {
  let lastLook = 0

  ctx.on('getWelcome', async () => {
    const state = (await BevyApi.getWelcome?.().catch(() => undefined)) ?? { terms: false, newProfile: false }
    ctx.send({ kind: 'welcome', ...state })
  })

  ctx.on('rerollLook', async () => {
    let n = lastLook
    while (n === lastLook) n = 1 + Math.floor(Math.random() * DEFAULT_LOOKS)
    lastLook = n
    const found = await getJson<DefaultLook>(`${await catalystBase()}/lambdas/profiles/default${n}`).catch((e: unknown) => {
      console.error(`[welcome] default look ${n}`, e)
      return undefined
    })
    const look = found?.avatars?.[0]?.avatar
    // a later reroll has replaced this one
    if (look?.bodyShape == null || n !== lastLook) return
    editLook({
      bodyShape: look.bodyShape,
      wearables: look.wearables ?? [],
      eyes: color(look.eyes),
      hair: color(look.hair),
      skin: color(look.skin),
      forceRender: []
    })
    await sendEquipped(ctx)
  })

  ctx.on('openLegal', async (msg) => {
    await openExternalUrl({ url: LEGAL[msg.doc] }).catch((e: unknown) => {
      console.error('[welcome] open', msg.doc, e)
    })
  })

  ctx.on('acceptWelcome', async (msg) => {
    try {
      // The terms first: a held new profile deploys with the first setAvatar after them.
      await BevyApi.acceptTerms?.()
      if (msg.name != null || hasDraft()) await deployLookAs(msg.name ?? getPlayer()?.name ?? '')
      ctx.send({ kind: 'welcomeAccepted', ok: true })
    } catch (e) {
      console.error('[welcome] accept failed', e)
      ctx.send({ kind: 'welcomeAccepted', ok: false, error: String(e) })
    }
  })
}
