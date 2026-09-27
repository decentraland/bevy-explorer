import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { BackpackPage } from '../features/backpack/BackpackPage'
import { fakeProfileState, fakeSession } from './harness'

function renderBackpack(over: Record<string, unknown> = {}) {
  const s = fakeSession()
  const backpack = { ...s.backpack, open: true, list: [], total: 0, ...over }
  render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
}

describe('backpack empty and loading states', () => {
  it('an empty category says so and links to the marketplace, with no detail panel', () => {
    renderBackpack()
    fireEvent.click(screen.getByRole('button', { name: 'Hat' }))
    expect(screen.getByText(/There are no items in this category\./)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Marketplace' })).toHaveAttribute('href', 'https://decentraland.org/marketplace')
    expect(screen.queryByText('No item selected')).toBeNull()
  })

  it('an empty search has its own message', async () => {
    renderBackpack()
    fireEvent.change(screen.getByPlaceholderText('Search item'), { target: { value: 'zzqq' } })
    expect(await screen.findByText(/meets this category or search criteria/, {}, { timeout: 2000 })).toBeInTheDocument()
  })

  it('shows a skeleton tile per slot while a page loads', () => {
    renderBackpack({ loading: true })
    expect(document.querySelectorAll('[aria-busy="true"] > span')).toHaveLength(16)
  })
})

describe('backpack tabs', () => {
  it('switching tab clears the selected item', () => {
    const s = fakeSession()
    const backpack = { ...s.backpack, open: true, list: [{ urn: 'urn:t', name: 'Blue Shirt', rarity: 'base', category: 'upper_body', equipped: false }], total: 1 }
    render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Blue Shirt' }))
    expect(screen.queryByText('No item selected')).toBeNull()
    fireEvent.click(screen.getByRole('tab', { name: /emotes/i }))
    expect(screen.getByText('No item selected')).toBeInTheDocument()
  })
})

describe('backpack outfits', () => {
  const renderOutfits = (outfitSlots: number) => {
    const s = fakeSession()
    const backpack = { ...s.backpack, open: true, outfitSlots }
    render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
    fireEvent.click(screen.getByRole('tab', { name: /saved outfits/i }))
  }

  it('without a NAME, five slots and the NAME banner', () => {
    renderOutfits(5)
    expect(screen.getAllByRole('button', { name: /^Save outfit in slot/ })).toHaveLength(5)
    expect(screen.getByText('Unlock 5 more Outfit slots by getting a NAME!')).toBeInTheDocument()
  })

  it('with a NAME, ten slots and no banner', () => {
    renderOutfits(10)
    expect(screen.getAllByRole('button', { name: /^Save outfit in slot/ })).toHaveLength(10)
    expect(screen.queryByText('Unlock 5 more Outfit slots by getting a NAME!')).toBeNull()
  })
})
