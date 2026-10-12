import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfileCard, openProfileCard } from '../features/profileCard/ProfileCard'
import { openPassport } from '../features/profile/Passport'
import { openFriendRequest } from '../features/friends/FriendRequestPopup'
import { SessionProvider } from '../features/session/SessionContext'
import { fakeSession } from './harness'
import { seedProfiles } from '../features/session/profileStore'
import { PopupHost, resetPopups } from '../design'
import type { EngineSession } from '../features/session/useEngineSession'

// View Profile opens the passport popup, Add Friend the request popup; stub both to assert the trigger.
vi.mock('../features/profile/Passport', () => ({ openPassport: vi.fn() }))
vi.mock('../features/friends/FriendRequestPopup', () => ({ openFriendRequest: vi.fn() }))

// COMPONENT: the smart <ProfileCard userId> resolves name/picture from the profile store and the
// relationship from the session, and renders the presentational card; openProfileCard mounts it as a popup.
afterEach(() => {
  resetPopups()
  vi.mocked(openPassport).mockClear()
})

function renderWithSession(node: React.ReactNode, mutate?: (s: EngineSession) => void): EngineSession {
  const s = fakeSession()
  mutate?.(s)
  render(<SessionProvider value={s}>{node}</SessionProvider>)
  return s
}

const card = (userId: string): React.JSX.Element => <ProfileCard userId={userId} x={10} y={10} onClose={vi.fn()} />

describe('ProfileCard container — resolution', () => {
  it('resolves name + avatar from the store and the relationship from the session by userId', () => {
    seedProfiles([{ address: '0xabc', name: 'Alice', picture: 'alice.png' }])
    renderWithSession(card('0xabc'), (s) => {
      s.friends.received = [{ id: 'r1', address: '0xabc', name: 'Alice' }] // → incoming
    })
    expect(screen.getByText('Alice')).toBeTruthy()
    expect(document.querySelector('img')?.getAttribute('src')).toBe('alice.png')
    // incoming relationship → Accept Friend
    expect(screen.getByRole('button', { name: 'Accept Friend' })).toBeTruthy()
  })

  it('resolves an address however the store learned it — here from a friends-list seed', () => {
    seedProfiles([{ address: '0xDEF', name: 'Bob', picture: 'bob.png' }]) // case-insensitive
    renderWithSession(card('0xdef'), (s) => {
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
  const asAlice = (): void => {
    seedProfiles([{ address: '0xabc', name: 'Alice' }]) // relationship none → ADD FRIEND
  }

  it('Add Friend opens the send-request popup for the user', async () => {
    renderWithSession(card('0xabc'), asAlice)
    await userEvent.click(screen.getByRole('button', { name: /Add Friend/i }))
    expect(openFriendRequest).toHaveBeenCalledWith('send', expect.objectContaining({ address: '0xabc' }))
  })

  it('Mention fires session.chat.mention', async () => {
    const s = renderWithSession(card('0xabc'), asAlice)
    await userEvent.click(screen.getByRole('button', { name: /Mention/i }))
    expect(s.chat.mention).toHaveBeenCalledWith('Alice')
  })

  it('View Profile opens the passport popup for the user', async () => {
    renderWithSession(card('0xabc'), asAlice)
    await userEvent.click(screen.getByRole('button', { name: /View Profile/i }))
    expect(openPassport).toHaveBeenCalledWith('0xabc')
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
    const confirm = screen.getByText('Are you sure you want to block Alice?').closest('[role="dialog"]') as HTMLElement
    await userEvent.click(within(confirm).getByRole('button', { name: 'BLOCK' }))
    await vi.waitFor(() => expect(s.friends.act).toHaveBeenCalled())
    expect(s.friends.act).toHaveBeenCalledWith('block', '0xabc')
  })
})

describe('openProfileCard', () => {
  it('mounts the card via the popup layer for the resolved user', () => {
    seedProfiles([{ address: '0xabc', name: 'Alice' }])
    renderWithSession(<PopupHost />)
    act(() => {
      openProfileCard('0xabc', 5, 5)
    })
    expect(screen.getByText('Alice')).toBeTruthy()
  })
})
