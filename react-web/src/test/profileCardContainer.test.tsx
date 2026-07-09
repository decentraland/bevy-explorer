import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfileCard, openProfileCard } from '../features/profileCard/ProfileCard'
import { SessionProvider } from '../features/session/SessionContext'
import { fakeSession } from './harness'
import { PopupHost, resetPopups } from '../design'
import type { EngineSession } from '../features/session/useEngineSession'

// COMPONENT: the smart <ProfileCard userId> resolves name/picture/relationship from the session
// (read via useSession) and renders the presentational card; openProfileCard mounts it as a popup.
afterEach(resetPopups)

function renderWithSession(node: React.ReactNode, mutate?: (s: EngineSession) => void): EngineSession {
  const s = fakeSession()
  mutate?.(s)
  render(<SessionProvider value={s}>{node}</SessionProvider>)
  return s
}

const card = (userId: string): React.JSX.Element => <ProfileCard userId={userId} x={10} y={10} onClose={vi.fn()} />

describe('ProfileCard container — resolution', () => {
  it('resolves name + avatar + relationship from the session by userId', () => {
    renderWithSession(card('0xabc'), (s) => {
      s.chat.members = [{ address: '0xabc', name: 'Alice', picture: 'alice.png' }]
      s.friends.received = [{ id: 'r1', address: '0xabc', name: 'Alice' }] // → incoming
    })
    expect(screen.getByText('Alice')).toBeTruthy()
    expect(document.querySelector('img')?.getAttribute('src')).toBe('alice.png')
    // incoming relationship → Accept / Reject CTA
    expect(screen.getByText(/accept/i)).toBeTruthy()
    expect(screen.getByText(/reject/i)).toBeTruthy()
  })

  it('resolves a friend not in the nearby roster from the friends list', () => {
    renderWithSession(card('0xdef'), (s) => {
      s.chat.members = [] // not nearby
      s.friends.list = [{ address: '0xdef', name: 'Bob', status: 'online', picture: 'bob.png' }]
    })
    expect(screen.getByText('Bob')).toBeTruthy()
    expect(document.querySelector('img')?.getAttribute('src')).toBe('bob.png')
  })

  it('falls back to the address as name with no avatar when the user is unknown', () => {
    const addr = '0xabc0000000000000000000000000000000000abc'
    renderWithSession(card(addr))
    expect(screen.getByRole('dialog')).toBeTruthy()
    expect(screen.getByText(addr)).toBeTruthy() // name falls back to the raw address
    expect(document.querySelector('img')).toBeNull() // Avatar with no picture → initials, no <img>
  })
})

describe('ProfileCard container — action wiring', () => {
  const asAlice = (s: EngineSession): void => {
    s.chat.members = [{ address: '0xabc', name: 'Alice' }] // relationship none → ADD FRIEND
  }

  it('ADD FRIEND fires the session friend action', async () => {
    const s = renderWithSession(card('0xabc'), asAlice)
    await userEvent.click(screen.getByRole('button', { name: /ADD FRIEND/i }))
    expect(s.friends.act).toHaveBeenCalledWith('request', '0xabc')
  })

  it('Mention fires session.chat.mention', async () => {
    const s = renderWithSession(card('0xabc'), asAlice)
    await userEvent.click(screen.getByRole('button', { name: /Mention/i }))
    expect(s.chat.mention).toHaveBeenCalledWith('Alice')
  })

  it('View Passport opens the session passport', async () => {
    const s = renderWithSession(card('0xabc'), asAlice)
    await userEvent.click(screen.getByRole('button', { name: /View Passport/i }))
    expect(s.openPassport).toHaveBeenCalledWith(expect.objectContaining({ address: '0xabc', name: 'Alice' }))
  })

  it('Block opens a confirm that fires the session friend action', async () => {
    const s = renderWithSession(
      <>
        {card('0xabc')}
        <PopupHost />
      </>,
      asAlice
    )
    await userEvent.click(screen.getByRole('button', { name: 'Block' }))
    const confirm = screen.getByText('Block Alice?').closest('[role="dialog"]') as HTMLElement
    await userEvent.click(within(confirm).getByRole('button', { name: 'Block' }))
    expect(s.friends.act).toHaveBeenCalledWith('block', '0xabc')
  })
})

describe('openProfileCard', () => {
  it('mounts the card via the popup layer, and a second call replaces it', () => {
    renderWithSession(<PopupHost />, (s) => {
      s.chat.members = [
        { address: '0xabc', name: 'Alice' },
        { address: '0xdef', name: 'Bob' }
      ]
    })
    act(() => {
      openProfileCard('0xabc', 5, 5)
    })
    expect(screen.getByText('Alice')).toBeTruthy()
    act(() => {
      openProfileCard('0xdef', 5, 5)
    })
    expect(screen.queryByText('Alice')).toBeNull() // replaced, not stacked
    expect(screen.getByText('Bob')).toBeTruthy()
  })
})
