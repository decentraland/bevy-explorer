import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Minimap } from '../features/minimap/Minimap'
import { fakeSession } from './harness'

// The minimap header: the scene name comes from the sceneInfo stream (see world.test.tsx for
// the wiring); this covers what the header actually renders, including the empty-parcel case.
function renderMinimap(sceneTitle: string): void {
  const session = fakeSession()
  render(<Minimap minimap={session.minimap} map={session.map} sceneTitle={sceneTitle} setEngineViewport={vi.fn()} />)
}

beforeEach(() => localStorage.clear())

describe('minimap header', () => {
  it('shows the scene the player is standing in', () => {
    renderMinimap('Genesis Plaza')
    expect(screen.getByText('Genesis Plaza')).toBeInTheDocument()
  })

  it('says the parcel is empty where nothing is deployed, not that the place is unknown', () => {
    renderMinimap('')
    expect(screen.getByText('Empty parcel')).toBeInTheDocument()
  })
})
