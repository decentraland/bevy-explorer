import { describe, it, expect, vi } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import { WearableCard } from '../design'
import { BackpackPage } from '../features/backpack/BackpackPage'
import { bodyShapesOf, fittingUrns, isCompatible, splitBodyShape } from '../engine/bodyShape'
import { enterAsGuest, fakeProfileState, fakeSession, renderSession } from './harness'

const MALE = 'urn:decentraland:off-chain:base-avatars:BaseMale'
const FEMALE = 'urn:decentraland:off-chain:base-avatars:BaseFemale'

// Audit backpack-emotes-4 / -13 (Unity BackpackItemView.IsCompatibleWithBodyShape).
describe('body shape', () => {
  it('reads the compatible body shapes from every representation', () => {
    expect(bodyShapesOf({ entity: { metadata: { data: { representations: [{ bodyShapes: [MALE] }, { bodyShapes: [FEMALE] }] } } } })).toEqual([MALE, FEMALE])
    expect(bodyShapesOf({})).toBeUndefined()
  })

  it('a body-shape item is deployed as the avatar base, not as a wearable', () => {
    expect(splitBodyShape(['urn:hat', FEMALE, 'urn:shoes'])).toEqual({ bodyShape: FEMALE, wearables: ['urn:hat', 'urn:shoes'] })
    expect(splitBodyShape(['urn:hat'])).toEqual({ bodyShape: undefined, wearables: ['urn:hat'] })
  })

  it('switching body shape keeps only the items the new shape can render', () => {
    const equipped = [
      { urn: 'urn:male-hat', category: 'hat', bodyShapes: [MALE] },
      { urn: 'urn:both-shoes', category: 'feet', bodyShapes: [MALE, FEMALE] },
      { urn: 'urn:unknown', category: 'unknown' }
    ]
    expect([...fittingUrns(equipped, FEMALE)]).toEqual(['urn:both-shoes', 'urn:unknown'])
  })

  it('an item fits when it lists the body shape (or lists none, or is a body shape)', () => {
    expect(isCompatible({ category: 'hat', bodyShapes: [MALE] }, FEMALE)).toBe(false)
    expect(isCompatible({ category: 'hat', bodyShapes: [MALE, FEMALE] }, FEMALE.toUpperCase())).toBe(true)
    expect(isCompatible({ category: 'hat' }, FEMALE)).toBe(true)
    expect(isCompatible({ category: 'body_shape', bodyShapes: [MALE] }, FEMALE)).toBe(true)
    expect(isCompatible({ category: 'hat', bodyShapes: [MALE] }, undefined)).toBe(true)
  })

  it('an incompatible card cannot be equipped and says why', () => {
    const onEquip = vi.fn()
    render(<WearableCard name="Beard" incompatible onEquip={onEquip} />)
    expect(screen.queryByText('EQUIP')).toBeNull()
    expect(screen.getByText('Incompatible with body shape')).toBeInTheDocument()
  })

  it('the backpack marks items that do not fit the current body shape', () => {
    const s = fakeSession()
    const backpack = {
      ...s.backpack,
      open: true,
      bodyShape: FEMALE,
      list: [{ urn: 'urn:beard', name: 'Beard', rarity: 'base', category: 'facial_hair', equipped: false, bodyShapes: [MALE] }]
    }
    render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'Beard' })).toHaveTextContent('Incompatible with body shape')
  })

  it('the session keeps the body shape the bridge reports', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.driver.emit({ kind: 'wearables', equipped: [], bodyShape: MALE }))
    expect(h.session().backpack.bodyShape).toBe(MALE)
  })
})
