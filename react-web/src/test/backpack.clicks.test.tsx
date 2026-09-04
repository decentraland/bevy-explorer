import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BackpackPage } from '../features/backpack/BackpackPage'
import type { Outfit, OutfitSlot, Wearable } from '../engine/protocol'
import type { BackpackState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

const wearable = (over: Partial<Wearable> = {}): Wearable => ({
  urn: 'urn:hat',
  name: 'Cool Hat',
  rarity: 'rare',
  category: 'hat',
  equipped: false,
  ...over
})

const outfitSlot = (slot: number, wearables: string[]): OutfitSlot => {
  const grey = { r: 0.5, g: 0.5, b: 0.5 }
  const outfit: Outfit = { bodyShape: '', eyes: { color: grey }, hair: { color: grey }, skin: { color: grey }, wearables, forceRender: [] }
  return { slot, outfit }
}

function renderBackpack(over: Partial<BackpackState> = {}): BackpackState {
  const backpack: BackpackState = { ...fakeSession().backpack, open: true, list: [wearable()], equip: vi.fn(), preview: vi.fn(), ...over }
  const emotes = { ...fakeSession().emotes, list: [{ slot: 1, urn: 'urn:wave', name: 'Wave' }] }
  render(
    <BackpackPage
      backpack={backpack}
      emotes={emotes}
      profile={{ data: null, open: false, toggle: vi.fn() }}
      onNavigate={vi.fn()}
      setEngineViewport={vi.fn()}
    />
  )
  return backpack
}

describe('backpack page clicks', () => {
  it('clicking a wearable card selects it (shows detail; does not equip or preview)', async () => {
    const backpack = renderBackpack()
    expect(screen.getByText('No item selected')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Cool Hat' }))
    // The detail panel now shows the item (its name renders as visible text there); the avatar is
    // left untouched — nothing is previewed or equipped.
    expect(screen.getByText('Cool Hat')).toBeInTheDocument()
    expect(vi.mocked(backpack.preview)).not.toHaveBeenCalled()
    expect(vi.mocked(backpack.equip)).not.toHaveBeenCalled()
  })

  it('double-clicking a wearable card equips it', async () => {
    const backpack = renderBackpack()
    await userEvent.dblClick(screen.getByRole('button', { name: 'Cool Hat' }))
    expect(vi.mocked(backpack.equip)).toHaveBeenCalledWith(['urn:hat'])
  })

  it('EQUIP persists the set and drops the preview', async () => {
    const backpack = renderBackpack()
    await userEvent.click(screen.getByRole('button', { name: 'EQUIP' }))
    expect(vi.mocked(backpack.equip)).toHaveBeenCalledWith(['urn:hat'])
    expect(vi.mocked(backpack.preview)).toHaveBeenCalledWith(null)
  })

  it('clicking the selected category again clears the filter back to all', async () => {
    const query = vi.fn()
    renderBackpack({ query })
    const hatCategory = screen.getByRole('button', { name: 'Hat' })
    await userEvent.click(hatCategory)
    expect(query).toHaveBeenLastCalledWith(expect.objectContaining({ category: 'hat' }))
    await userEvent.click(hatCategory)
    expect(query).toHaveBeenLastCalledWith(expect.objectContaining({ category: undefined }))
  })

  it('an equipped removable category slot has a hover unequip button that unequips it', async () => {
    const backpack = renderBackpack({ list: [], equipped: [wearable({ category: 'hat', equipped: true })] })
    await userEvent.click(screen.getByRole('button', { name: 'Unequip Hat' }))
    expect(vi.mocked(backpack.equip)).toHaveBeenCalledWith([])
  })

  it('a required category (eyes) shows no unequip button even when equipped', () => {
    renderBackpack({ list: [], equipped: [wearable({ urn: 'urn:eyes', category: 'eyes', equipped: true })] })
    expect(screen.queryByRole('button', { name: 'Unequip Eyes' })).toBeNull()
  })

  it('outfits: the outfit matching the current look is marked equipped', async () => {
    renderBackpack({
      list: [],
      equipped: [wearable({ urn: 'urn:hat', category: 'hat' }), wearable({ urn: 'urn:shirt', category: 'upper_body' })],
      outfits: [outfitSlot(0, ['urn:hat', 'urn:shirt']), outfitSlot(1, ['urn:other'])]
    })
    await userEvent.click(screen.getByRole('button', { name: /saved outfits/i }))
    // Only slot 0 (Outfit 1) equals the equipped set → exactly one equipped dot.
    expect(document.querySelectorAll('[class*="outfitDot"]')).toHaveLength(1)
  })

  it('outfits: selecting shows the outfit wearables in the detail panel without equipping or previewing', async () => {
    const backpack = renderBackpack({
      list: [],
      equipped: [],
      outfits: [outfitSlot(0, ['urn:hat', 'urn:shirt', 'urn:pants'])]
    })
    await userEvent.click(screen.getByRole('button', { name: /saved outfits/i }))
    await userEvent.click(screen.getByRole('button', { name: 'Outfit 1' }))
    // The detail panel shows every wearable of the outfit (the card composite shows only four).
    expect(document.querySelectorAll('[class*="outfitDetailGrid"] img')).toHaveLength(3)
    expect(vi.mocked(backpack.equipOutfit)).not.toHaveBeenCalled()
    expect(vi.mocked(backpack.preview)).not.toHaveBeenCalled()
  })

  it('outfits: double-clicking an outfit equips it', async () => {
    const backpack = renderBackpack({ list: [], equipped: [], outfits: [outfitSlot(0, ['urn:hat'])] })
    await userEvent.click(screen.getByRole('button', { name: /saved outfits/i }))
    await userEvent.dblClick(screen.getByRole('button', { name: 'Outfit 1' }))
    expect(vi.mocked(backpack.equipOutfit)).toHaveBeenCalledWith(0)
  })

  it('switching to the Emotes tab shows the equipped emotes', async () => {
    renderBackpack()
    await userEvent.click(screen.getByRole('button', { name: 'Emotes' }))
    expect(screen.getByText('Wave')).toBeInTheDocument()
  })

  it('opens directly on the Emotes tab when initialTab="emotes" (the wheel\'s "Customise [E]")', () => {
    const backpack: BackpackState = { ...fakeSession().backpack, open: true, list: [wearable()], equip: vi.fn(), preview: vi.fn() }
    const emotes = { ...fakeSession().emotes, list: [{ slot: 1, urn: 'urn:wave', name: 'Wave' }] }
    render(
      <BackpackPage
        backpack={backpack}
        emotes={emotes}
        profile={{ data: null, open: false, toggle: vi.fn() }}
        onNavigate={vi.fn()}
        setEngineViewport={vi.fn()}
        initialTab="emotes"
      />
    )
    // Emotes content is shown with no tab click; the wearable grid (Cool Hat) is not.
    expect(screen.getByText('Wave')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Cool Hat' })).not.toBeInTheDocument()
  })
})
