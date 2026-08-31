import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import { Passport, openPassport } from '../features/profile/Passport'
import { SessionProvider } from '../features/session/SessionContext'
import { fakeSession } from './harness'
import { PopupHost, resetPopups } from '../design'
import type { EngineSession } from '../features/session/useEngineSession'
import type { Profile } from '../engine/protocol'

// COMPONENT: the smart <Passport userId> fetches the rich profile on open, renders identity-only from
// the session until it lands, and shows the presentational ProfilePassport; openPassport mounts it.
afterEach(resetPopups)

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
    const s = renderWithSession(<Passport userId="0xabc" onClose={vi.fn()} />, (sess) => {
      sess.userProfiles['0xabc'] = RICH
    })
    expect(s.requestUserProfile).toHaveBeenCalledWith('0xabc')
    expect(screen.getByText('gm from the plaza')).toBeTruthy() // rich field from the fetched profile
    expect(screen.getByText('5 Mutual')).toBeTruthy()
  })

  it('renders identity-only from the roster while the fetch is in flight', () => {
    const s = renderWithSession(<Passport userId="0xabc" onClose={vi.fn()} />, (sess) => {
      sess.chat.members = [{ address: '0xabc', name: 'Alice', picture: 'a.png' }] // no userProfiles entry yet
    })
    expect(s.requestUserProfile).toHaveBeenCalledWith('0xabc')
    expect(screen.getByText('Alice')).toBeTruthy() // resolved from the roster
  })

  it('openPassport mounts the passport via the popup layer', () => {
    renderWithSession(<PopupHost />, (s) => {
      s.userProfiles['0xabc'] = RICH
    })
    act(() => {
      openPassport('0xabc')
    })
    expect(screen.getByText('gm from the plaza')).toBeTruthy()
  })
})
