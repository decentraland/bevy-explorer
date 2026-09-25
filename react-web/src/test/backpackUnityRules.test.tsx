import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { BackpackPage } from '../features/backpack/BackpackPage'
import type { Wearable } from '../engine/protocol'
import { fakeProfileState, fakeSession } from './harness'

const MALE = 'urn:decentraland:off-chain:base-avatars:BaseMale'
const FEMALE = 'urn:decentraland:off-chain:base-avatars:BaseFemale'
const item = (urn: string, name: string, category: string, extra: Partial<Wearable> = {}): Wearable =>
  ({ urn, name, rarity: 'base', category, equipped: false, ...extra })

function renderBackpack(list: Wearable[], over: Record<string, unknown> = {}) {
  const s = fakeSession()
  const equipped = list.filter((w) => w.equipped)
  const backpack = { ...s.backpack, open: true, bodyShape: MALE, list, equipped, total: list.length, equip: vi.fn(), ...over }
  render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
  return backpack
}

// Unity rules: BackpackItemView.cs:209-215, AvatarView.cs:16-44,
// CheckOutfitsBannerVisibilityCommand.cs.
describe('backpack follows Unity', () => {
  it('double-click only equips, and never an item that does not fit the body shape', () => {
    const bp = renderBackpack([
      item('urn:hat', 'Cowboy Hat', 'hat', { equipped: true }),
      item('urn:cap', 'Sun Glasses', 'eyewear'),
      item('urn:skirt', 'Long Skirt', 'lower_body', { bodyShapes: [FEMALE] })
    ])
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Cowboy Hat' }))
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Long Skirt' }))
    expect(bp.equip).not.toHaveBeenCalled()
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Sun Glasses' }))
    expect(bp.equip).toHaveBeenCalledWith(['urn:hat', 'urn:cap'])
  })
})
