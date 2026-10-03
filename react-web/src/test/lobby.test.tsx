import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { LobbyHome } from '../features/lobby/LobbyHome'
import { SessionProvider } from '../features/session/SessionContext'
import { fakeSession } from './harness'

vi.mock('../features/lobby/lobbyApi', () => ({
  fetchHomePlace: vi.fn(async () => null),
  fetchRecents: vi.fn(async () => []),
  fetchHighlighted: vi.fn(async () => []),
  fetchLiveEvents: vi.fn(async () => []),
  fetchLivePlaces: vi.fn(async () => []),
  eventPeople: vi.fn(() => 0)
}))

afterEach(() => {
  vi.restoreAllMocks()
  vi.clearAllMocks()
})

describe('lobby controls', () => {
  it('disables Customize until the player is ready', async () => {
    const session = { ...fakeSession(), playerReady: false }
    const view = (): React.JSX.Element => (
      <SessionProvider value={session}>
        <LobbyHome onPick={session.pickDestination} setEngineViewport={session.setEngineViewport} />
      </SessionProvider>
    )
    const { rerender } = render(view())
    expect(screen.getByRole('button', { name: 'Customize' })).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: 'Customize' }))
    expect(session.backpack.toggle).not.toHaveBeenCalled()
    session.playerReady = true
    rerender(view())
    fireEvent.click(screen.getByRole('button', { name: 'Customize' }))
    expect(session.backpack.toggle).toHaveBeenCalledOnce()
    await waitFor(() => expect(screen.getByRole('button', { name: /jump in/i })).toBeInTheDocument())
  })
})
