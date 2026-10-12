import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { Wearable } from '../engine/protocol'
import { SAVE_FAILED_MESSAGE } from '../features/session/useEngineSession'
import { enterAsGuest, renderSession } from './harness'

const wearable = (urn: string, category = 'hat'): Wearable => ({ urn, name: urn, rarity: 'common', category, equipped: true })

// The Backpack deploys its look when it closes, like Unity (BackpackController.Deactivate), instead of
// on every equip. Audit backpack-emotes-1: a failed deploy was only console.error'd; Unity warns.
describe('backpack deploy on close', () => {
  it('equipping deploys nothing until the Backpack closes', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().backpack.toggle())
    act(() => h.session().backpack.equip(['urn:hat-b']))
    act(() => h.session().emotes.equip(0, 'urn:dance'))
    act(() => h.session().backpack.equipOutfit(1))
    expect(h.driver.sentOf('commitAvatar')).toHaveLength(0)
    act(() => h.session().backpack.toggle())
    expect(h.driver.sentOf('commitAvatar')).toHaveLength(1)
  })

  it('opening another page closes the Backpack and deploys too', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().backpack.toggle())
    act(() => h.session().settings.toggle())
    expect(h.session().backpack.open).toBe(false)
    expect(h.driver.sentOf('commitAvatar')).toHaveLength(1)
  })

  it('a failed deploy says so and keeps the look', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().backpack.toggle())
    act(() => h.session().backpack.equip(['urn:hat-b']))
    act(() => h.driver.emit({ kind: 'wearables', equipped: [wearable('urn:hat-b')] }))
    act(() => h.session().backpack.toggle())
    act(() => h.driver.emit({ kind: 'avatarSaveFailed', message: 'failed to deploy to server.' }))
    expect(h.session().backpack.saveError).toBe(SAVE_FAILED_MESSAGE)
    expect(h.session().backpack.equipped.map((w) => w.urn)).toEqual(['urn:hat-b'])
  })

  it('Retry deploys again; Revert asks the bridge for the last deployed look', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.driver.emit({ kind: 'avatarSaveFailed', message: 'x' }))
    act(() => h.session().backpack.retrySave())
    expect(h.session().backpack.saveError).toBeNull()
    expect(h.driver.sentOf('commitAvatar')).toHaveLength(1)

    act(() => h.driver.emit({ kind: 'avatarSaveFailed', message: 'x' }))
    const reads = h.driver.sentOf('getWearables').length
    act(() => h.session().backpack.revertSave())
    expect(h.session().backpack.saveError).toBeNull()
    expect(h.driver.sentOf('revertAvatar')).toHaveLength(1)
    expect(h.driver.sentOf('getWearables')).toHaveLength(reads + 1)
  })
})
