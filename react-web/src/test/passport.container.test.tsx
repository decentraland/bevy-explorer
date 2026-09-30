import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest'
import { render, screen, act, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Passport, openPassport } from '../features/profile/Passport'
import { SessionProvider } from '../features/session/SessionContext'
import { fakeProfileState, fakeSession } from './harness'
import { receiveProfile, seedProfiles, setProfileRequester } from '../features/session/profileStore'
import { PopupHost, closeTopPopup, resetPopups } from '../design'
import type { EngineSession } from '../features/session/useEngineSession'
import type { Profile } from '../engine/protocol'

// COMPONENT: the smart <Passport userId> fetches the rich profile on open, renders identity-only from
// the profile store until it lands, and shows the presentational ProfilePassport; openPassport mounts it.
afterEach(resetPopups)

const request = vi.fn()
beforeEach(() => {
  request.mockClear()
  setProfileRequester(request)
})

function renderWithSession(node: React.ReactNode, mutate?: (s: EngineSession) => void): EngineSession {
  const s = fakeSession()
  mutate?.(s)
  render(<SessionProvider value={s}>{node}</SessionProvider>)
  return s
}

const RICH: Profile = {
  address: '0xabc',
  name: 'Alice',
  picture: 'a.png',
  hasClaimedName: true,
  isGuest: false,
  description: 'gm from the plaza',
  mutuals: 5
}

describe('Passport container', () => {
  it('fetches the rich profile on open and renders it', () => {
    receiveProfile('0xabc', RICH)
    renderWithSession(<Passport userId="0xabc" onClose={vi.fn()} />)
    expect(request).toHaveBeenCalledWith('0xabc', true)
    expect(screen.getByText('gm from the plaza')).toBeTruthy() // rich field from the fetched profile
    expect(screen.getByText('5 Mutual')).toBeTruthy()
  })

  it('renders identity-only from a roster seed while the fetch is in flight', () => {
    seedProfiles([{ address: '0xabc', name: 'Alice', picture: 'a.png' }]) // no engine profile yet
    renderWithSession(<Passport userId="0xabc" onClose={vi.fn()} />)
    expect(request).toHaveBeenCalledWith('0xabc', true)
    expect(screen.getByText('Alice')).toBeTruthy() // resolved from the roster
  })

  it('openPassport mounts the passport via the popup layer', () => {
    receiveProfile('0xabc', RICH)
    renderWithSession(<PopupHost />)
    act(() => {
      openPassport('0xabc')
    })
    expect(screen.getByText('gm from the plaza')).toBeTruthy()
  })
})

// The passport is the HUD's one popup that can hold unsaved work, so it is where the popup layer's
// close guard is exercised: a stray backdrop click is refused outright, while the deliberate paths
// (its ×, and the engine-resolved Cancel action) ask the same question.
describe('closing a passport with unsaved edits', () => {
  const openSelfPassport = async (): Promise<void> => {
    receiveProfile('0xme', { ...RICH, address: '0xme', name: 'Me' })
    renderWithSession(<PopupHost />, (s) => {
      s.profile = fakeProfileState({ data: { ...RICH, address: '0xme', name: 'Me' } })
    })
    act(() => {
      openPassport('0xme')
    })
    await userEvent.click(screen.getByRole('button', { name: 'Edit profile' }))
    await userEvent.type(screen.getByLabelText('About me'), '!')
  }

  const backdrop = (): HTMLElement => document.querySelector('[class*="backdrop"]') as HTMLElement

  it('refuses a stray backdrop click outright — no dialog, nothing lost', async () => {
    await openSelfPassport()
    fireEvent.click(backdrop())
    expect(screen.queryByText('Discard changes?')).toBeNull()
    expect(screen.getByLabelText('About me')).toBeTruthy()
  })

  it('has no × to press while editing — that corner is CANCEL', async () => {
    await openSelfPassport()
    expect(screen.queryByRole('button', { name: 'Close' })).toBeNull()
    expect(screen.getByRole('button', { name: 'CANCEL' })).toBeTruthy()
  })

  it('asks on the Cancel action too — Escape must not be the one path that bins an edit', async () => {
    await openSelfPassport()
    act(() => closeTopPopup())
    expect(await screen.findByText('Discard changes?')).toBeTruthy()

    // Escape ON the confirm resolves it as dismissed, which is the non-destructive answer.
    act(() => closeTopPopup())
    expect(screen.queryByText('Discard changes?')).toBeNull()
    expect(screen.getByLabelText('About me')).toBeTruthy()
  })

  it('closes the whole passport on Discard', async () => {
    await openSelfPassport()
    act(() => closeTopPopup())
    await userEvent.click(await screen.findByRole('button', { name: 'Discard' }))
    expect(screen.queryByLabelText('About me')).toBeNull()
  })

  it('closes without a word when nothing has been edited', async () => {
    receiveProfile('0xabc', RICH)
    renderWithSession(<PopupHost />)
    act(() => {
      openPassport('0xabc')
    })
    act(() => closeTopPopup())
    expect(screen.queryByText('gm from the plaza')).toBeNull()
  })
})
