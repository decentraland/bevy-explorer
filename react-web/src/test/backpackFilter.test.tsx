import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { BackpackPage } from '../features/backpack/BackpackPage'
import { fakeProfileState, fakeSession } from './harness'

function renderBackpack() {
  const s = fakeSession()
  const backpack = { ...s.backpack, open: true, query: vi.fn() }
  render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
  return backpack
}

describe('backpack filter & sort', () => {
  it('sorts newest first by default', () => {
    const bp = renderBackpack()
    expect(bp.query).toHaveBeenLastCalledWith(expect.objectContaining({ orderBy: 'date', direction: 'desc', collectiblesOnly: false }))
  })

  it('offers the six sorts and two views, and re-queries with the choice', () => {
    const bp = renderBackpack()
    fireEvent.click(screen.getByRole('button', { name: /filter & sort/i }))
    const labels = screen.getAllByRole('menuitemradio').map((el) => el.textContent)
    expect(labels).toEqual(['Newest', 'Oldest', 'Rarest', 'Less rare', 'Name A-Z', 'Name Z-A', 'All Items', 'Collectibles only'])
    expect(screen.getByRole('menuitemradio', { name: 'Newest' })).toHaveAttribute('aria-checked', 'true')
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Name A-Z' }))
    expect(bp.query).toHaveBeenLastCalledWith(expect.objectContaining({ orderBy: 'name', direction: 'asc' }))
    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Collectibles only' }))
    expect(bp.query).toHaveBeenLastCalledWith(expect.objectContaining({ collectiblesOnly: true }))
  })
})
