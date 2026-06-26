import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MapPage } from '../features/map/MapPage'
import type { MapState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function renderMap(): MapState {
  const map: MapState = { ...fakeSession().map, open: true, teleport: vi.fn(), toggle: vi.fn() }
  render(<MapPage map={map} profile={{ data: null, open: false, toggle: vi.fn() }} onNavigate={vi.fn()} />)
  return map
}

afterEach(() => {
  vi.restoreAllMocks()
})

describe('map page', () => {
  it('zoom + / − controls are clickable', async () => {
    renderMap()
    await userEvent.click(screen.getByRole('button', { name: '+' }))
    await userEvent.click(screen.getByRole('button', { name: '−' }))
  })

  it('category chips switch the active filter', async () => {
    renderMap()
    await userEvent.click(screen.getByRole('button', { name: 'Social' }))
    expect(screen.getByRole('button', { name: 'Social' })).toBeInTheDocument()
  })

  it('clicking a parcel then JUMP IN teleports there and closes the map', async () => {
    const place = {
      title: 'Plaza',
      description: 'd',
      owner: null,
      image: '',
      base_position: '10,-5',
      positions: ['10,-5'],
      user_count: 1,
      like_rate: null
    }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ json: async () => ({ data: [place] }) }))
    const map = renderMap()

    // A non-drag click on the atlas → fetchPlace → place panel.
    const atlas = screen.getByAltText('Decentraland map')
    fireEvent.mouseDown(atlas, { clientX: 5, clientY: 5 })
    fireEvent.mouseUp(atlas, { clientX: 5, clientY: 5 })

    await userEvent.click(await screen.findByRole('button', { name: 'JUMP IN' }))
    expect(vi.mocked(map.teleport)).toHaveBeenCalledWith(10, -5)
    expect(vi.mocked(map.toggle)).toHaveBeenCalledTimes(1)
  })
})
