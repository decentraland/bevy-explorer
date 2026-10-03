import { readFileSync } from 'node:fs'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { BackpackModal } from '../features/backpack/BackpackModal'
import modalStyles from '../features/backpack/BackpackModal.module.css'
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

  it('leaves the avatar cutout outside both overlay hit regions and keeps Close clickable', async () => {
    const sheet = document.createElement('style')
    const css = readFileSync('src/features/backpack/BackpackModal.module.css', 'utf8')
    sheet.textContent = css.replace(/\.([a-zA-Z]\w*)/g, (selector, name: string) => modalStyles[name] ? `.${modalStyles[name]}` : selector)
    document.head.append(sheet)
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(100, 100, 1600, 850))
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(1920)
    const session = fakeSession()
    session.backpack.open = true
    const view = (): React.JSX.Element => (
      <SessionProvider value={session}>
        <LobbyHome onPick={session.pickDestination} setEngineViewport={session.setEngineViewport} />
        <BackpackModal onClose={session.backpack.toggle}><div /></BackpackModal>
      </SessionProvider>
    )
    try {
      const { rerender } = render(view())
      session.avatarPreviewRect = { x: 90, y: 130, width: 600, height: 800 }
      rerender(view())
      const frame = screen.getByRole('dialog', { name: 'Backpack' })
      const dim = frame.previousElementSibling as HTMLElement
      const lobby = screen.getByRole('button', { name: 'Customize' }).parentElement!
      expect(getComputedStyle(frame.parentElement!).pointerEvents).toBe('none')
      expect(getComputedStyle(frame).pointerEvents).toBe('none')
      expect(dim.style.clipPath).toMatch(/^polygon\(evenodd,/)
      expect(lobby.style.clipPath).toMatch(/^polygon\(evenodd,/)
      fireEvent.click(dim, { clientX: 1500, clientY: 900 })
      expect(session.backpack.toggle).not.toHaveBeenCalled()
      fireEvent.click(dim, { clientX: 50, clientY: 50 })
      expect(session.backpack.toggle).toHaveBeenCalledOnce()
      const close = screen.getByRole('button', { name: 'Close' })
      expect(getComputedStyle(close).pointerEvents).toBe('auto')
      fireEvent.click(close)
      expect(session.backpack.toggle).toHaveBeenCalledTimes(2)
      await waitFor(() => expect(screen.getByRole('button', { name: /jump in/i })).toBeInTheDocument())
    } finally {
      sheet.remove()
    }
  })
})
