import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfilePassport } from '../features/profile/ProfilePassport'
import type { Profile } from '../engine/protocol'

const profile: Profile = {
  address: '0xkurd000000000000000000000000000000006b635',
  name: 'kurd',
  picture: 'k.png',
  hasClaimedName: true,
  isGuest: false,
  description: 'old gamer in dcl',
  mutuals: 30,
  links: [{ title: 'x account', url: 'https://x.com/kurd' }],
  badges: [{ id: 'b1', name: 'Festive Trail' }],
  info: { gender: 'Male', realName: 'mohammad', language: 'Persian' }
}

describe('profile passport', () => {
  it('renders the overview: name, about, fields, links, mutuals', () => {
    render(<ProfilePassport profile={profile} onClose={vi.fn()} />)
    expect(screen.getByText('kurd')).toBeInTheDocument()
    expect(screen.getByText('old gamer in dcl')).toBeInTheDocument()
    expect(screen.getByText('mohammad')).toBeInTheDocument() // Real Name field value
    expect(screen.getByText('30 Mutual')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /x account/i })).toHaveAttribute('href', 'https://x.com/kurd')
  })

  it('ADD FRIEND when not a friend; FRIEND (disabled) when already', async () => {
    const onAddFriend = vi.fn()
    const { rerender } = render(<ProfilePassport profile={profile} onAddFriend={onAddFriend} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'ADD FRIEND' }))
    expect(onAddFriend).toHaveBeenCalledWith(profile.address)

    rerender(<ProfilePassport profile={profile} relationship="friend" onAddFriend={onAddFriend} onClose={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'FRIEND' })).toBeDisabled()
  })

  it('Add Friend requests, then flips to REQUESTED (optimistic feedback)', async () => {
    const onAddFriend = vi.fn()
    render(<ProfilePassport profile={profile} onAddFriend={onAddFriend} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'ADD FRIEND' }))
    expect(onAddFriend).toHaveBeenCalledWith(profile.address)
    expect(screen.getByRole('button', { name: 'REQUESTED' })).toBeDisabled()
  })

  it('shows REQUESTED (not Add Friend) when a request is already pending', () => {
    render(<ProfilePassport profile={profile} relationship="requested" onClose={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'REQUESTED' })).toBeDisabled()
    expect(screen.queryByRole('button', { name: 'ADD FRIEND' })).toBeNull()
  })

  it('hides the friend action for an incoming request (would duplicate-request otherwise)', () => {
    render(<ProfilePassport profile={profile} relationship="incoming" onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /FRIEND/i })).toBeNull()
  })

  it('hides the friend action for a blocked user', () => {
    render(<ProfilePassport profile={profile} relationship="blocked" onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /FRIEND/i })).toBeNull()
  })

  it('hides the friend action on your own passport (isSelf)', () => {
    render(<ProfilePassport profile={profile} isSelf onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /FRIEND/i })).toBeNull()
  })

  it('uses the full-body snapshot as the avatar when present', () => {
    render(<ProfilePassport profile={{ ...profile, bodyImage: 'https://x/body.png' }} onClose={vi.fn()} />)
    expect(screen.getAllByRole('img').some((i) => i.getAttribute('src') === 'https://x/body.png')).toBe(true)
  })

  it('close button closes', async () => {
    const onClose = vi.fn()
    render(<ProfilePassport profile={profile} onClose={onClose} />)
    await userEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('Escape closes the passport', () => {
    const onClose = vi.fn()
    render(<ProfilePassport profile={profile} onClose={onClose} />)
    fireEvent.keyDown(window, { key: 'Escape' })
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('Photos tab renders the camera-reel photos', async () => {
    render(<ProfilePassport profile={{ ...profile, photos: ['https://x/p1.png', 'https://x/p2.png'] }} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'PHOTOS' }))
    expect(screen.getAllByRole('link').some((a) => a.getAttribute('href') === 'https://x/p1.png')).toBe(true)
  })

  it('shows a graceful empty state when the user has no details', () => {
    render(<ProfilePassport profile={{ address: '0xnobody', name: 'Nobody', hasClaimedName: false, isGuest: false }} onClose={vi.fn()} />)
    expect(screen.getByText(/no details to show/i)).toBeInTheDocument()
  })

  it('switches tabs (Photos shows empty state)', async () => {
    render(<ProfilePassport profile={profile} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'PHOTOS' }))
    expect(screen.getByText(/No photos shared yet/i)).toBeInTheDocument()
  })
})
