// Welcome page: where the terms are accepted, and an account without a profile settles the name and
// look its new profile deploys with. The looks come from the page (its fetches work before sign-in,
// the scene's signed ones don't): before sign-in the stage wears one as a stand-in, signed in it is
// the Backpack's draft (./avatarDraft), so the stage wears each pick before anything deploys.
//   from: getWelcome / expectLook / showLook / acceptWelcome (the page), BevyApi.getWelcome
//   to:   welcome / welcomeAccepted / wearables (the page), BevyApi.setAvatar + acceptTerms,
//         RestrictedActions.openExternalUrl (openLegal)
import { AvatarEquippedData, engine } from '@dcl/sdk/ecs'
import { getPlayer } from '@dcl/sdk/players'
import { openExternalUrl } from '~system/RestrictedActions'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'
import { deployLookAs, editLook, hasDraft, setStandInLook } from './avatarDraft'
import { expectLook, syncShape, waitFrames } from './avatarPreview'
import type { AvatarLook } from '../../../src/engine/avatarEquip'
import { sendEquipped } from './wearables'

const LEGAL = {
  terms: 'https://decentraland.org/terms/',
  privacy: 'https://decentraland.org/privacy/'
}
// Frames to wait for a signed-in player's equipped data before the accept deploys the look
const PLAYER_LOOK_FRAMES = 600

export function registerWelcome(ctx: Ctx): void {
  ctx.on('getWelcome', async () => {
    const state = (await BevyApi.getWelcome?.().catch(() => undefined)) ?? { terms: false, newProfile: false }
    ctx.send({ kind: 'welcome', ...state })
  })

  // A curated default look, as the page fetched it, on the stage: signed in, on the draft (the
  // page's equipped set follows); not yet, as the stand-in. Applied a frame later, so the picks
  // queued while the scene was not yet up (the page's rerolls before the engine ran) collapse to
  // the last.
  ctx.on('expectLook', () => {
    expectLook()
  })
  let queuedShow: { look: number; avatar: AvatarLook } | null = null
  ctx.on('showLook', (msg) => {
    queuedShow = { look: msg.look, avatar: msg.avatar }
  })
  ctx.push(() => {
    if (queuedShow == null) return
    const { look: n, avatar: look } = queuedShow
    queuedShow = null
    if (getPlayer() == null) {
      setStandInLook(look)
      syncShape(n)
      return
    }
    editLook({ bodyShape: look.bodyShape, wearables: look.wearables, eyes: look.eyes, hair: look.hair, skin: look.skin, forceRender: [] })
    syncShape(n)
    sendEquipped(ctx).catch((e: unknown) => {
      console.error('[welcome] equipped', e)
    })
  })

  ctx.on('openLegal', async (msg) => {
    await openExternalUrl({ url: LEGAL[msg.doc] }).catch((e: unknown) => {
      console.error('[welcome] open', msg.doc, e)
    })
  })

  ctx.on('acceptWelcome', async (msg) => {
    try {
      // The terms first: a held new profile deploys with the first setAvatar after them (the guest
      // start page's accept carries its name for that).
      await BevyApi.acceptTerms?.()
      if (msg.name != null || hasDraft()) {
        // the look deploys as the player wears it, which reaches the scene after the player's
        // identity (the sign-in's accept can land between): deployed before, it would be bare
        for (let waited = 0; !AvatarEquippedData.has(engine.PlayerEntity) && waited < PLAYER_LOOK_FRAMES; waited++) await waitFrames(1)
        await deployLookAs(msg.name ?? getPlayer()?.name ?? '')
      }
      ctx.send({ kind: 'welcomeAccepted', ok: true })
    } catch (e) {
      console.error('[welcome] accept failed', e)
      ctx.send({ kind: 'welcomeAccepted', ok: false, error: String(e) })
    }
  })
}
