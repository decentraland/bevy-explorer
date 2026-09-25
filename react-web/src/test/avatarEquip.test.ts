import { describe, it, expect } from 'vitest'
import { equipPayload } from '../engine/avatarEquip'

// Audit backpack-emotes-2: every wearable/emote equip sent forceRender: [], erasing overrides.
describe('equip payload', () => {
  const me = { wearables: ['urn:hat'], emotes: ['wave'], forceRender: ['hair'] }

  it('keeps force-render overrides when equipping wearables', () => {
    expect(equipPayload(me, { wearableUrns: ['urn:cap'] })).toEqual({ wearableUrns: ['urn:cap'], emoteUrns: ['wave'], forceRender: ['hair'] })
  })

  it('keeps them when assigning an emote slot', () => {
    expect(equipPayload(me, { emoteUrns: ['dance'] }).forceRender).toEqual(['hair'])
  })

  it('sends none when the player has none', () => {
    expect(equipPayload(null, { wearableUrns: [] }).forceRender).toEqual([])
  })
})
