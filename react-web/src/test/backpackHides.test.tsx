import { describe, it, expect, vi } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BackpackPage } from '../features/backpack/BackpackPage'
import type { Wearable } from '../engine/protocol'
import type { BackpackState } from '../features/session/useEngineSession'
import { fakeProfileState, fakeSession } from './harness'

const hat: Wearable = { urn: 'urn:hat', name: 'Top Hat', rarity: 'rare', category: 'hat', equipped: true, hides: ['hair'] }
const hair: Wearable = { urn: 'urn:hair', name: 'Long Hair', rarity: 'rare', category: 'hair', equipped: true }

function renderBackpack(over: Partial<BackpackState> = {}): BackpackState {
  const s = fakeSession()
  const backpack: BackpackState = { ...s.backpack, open: true, list: [hat], total: 1, equipped: [hat, hair], setForceRender: vi.fn(), ...over }
  render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
  return backpack
}

describe('backpack hides', () => {
  it('a hidden slot says what hides it and offers to show it anyway', async () => {
    const backpack = renderBackpack()
    const slot = screen.getByRole('button', { name: 'Hair' })
    expect(within(slot).getByRole('tooltip')).toHaveTextContent('Hidden by Hat')
    await userEvent.click(within(slot).getByRole('button', { name: 'Show Hair' }))
    expect(vi.mocked(backpack.setForceRender)).toHaveBeenCalledWith(['hair'])
  })

  it('a slot shown anyway keeps its toggle, which hides it again', async () => {
    const backpack = renderBackpack({ forceRender: ['hair'] })
    await userEvent.click(screen.getByRole('button', { name: 'Hide Hair' }))
    expect(vi.mocked(backpack.setForceRender)).toHaveBeenCalledWith([])
  })

  it('slots that nothing hides have no toggle', () => {
    renderBackpack()
    expect(screen.queryByRole('button', { name: /^(Show|Hide) Hat$/ })).toBeNull()
  })

  it('the info panel lists what the selected item hides', async () => {
    renderBackpack()
    await userEvent.click(screen.getByRole('button', { name: 'Top Hat' }))
    const label = screen.getByText('HIDES')
    expect(label.nextElementSibling).toHaveTextContent(/^Hair$/)
  })
})
