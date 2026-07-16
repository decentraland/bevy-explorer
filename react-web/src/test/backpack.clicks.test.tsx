import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BackpackPage } from '../features/backpack/BackpackPage'
import type { Wearable } from '../engine/protocol'
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
