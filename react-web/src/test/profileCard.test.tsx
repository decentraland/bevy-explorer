import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfileCard } from '../features/chat/ProfileCard'

// DOMAIN: profile-card — the shared popover (chat / friends / world avatar click). Covers the
// action set ported from bevy-ui-scene's profile-menu: relationship-driven friend CTA (Add /
// Accept+Reject / Requested / Unblock), View Passport, Mention, Block. (Report was removed until a
// moderation endpoint exists, and "Invite to Community" is parked for the communities feature — see backlog.md.)
const ALICE = { address: '0xalice', name: 'Alice' }

function renderCard(props: Partial<React.ComponentProps<typeof ProfileCard>> = {}): { onClose: () => void } {
  const onClose = vi.fn()
  render(<ProfileCard user={ALICE} x={20} y={20} me={{ address: '0xme' }} onClose={onClose} {...props} />)
  return { onClose }
}

describe('profile-card actions', () => {
  it('an incoming request shows Accept / Reject wired to the unified friend action', async () => {
    const onFriendAction = vi.fn()
    renderCard({ relationship: 'incoming', onFriendAction })
    expect(screen.queryByRole('button', { name: /ADD FRIEND/i })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'ACCEPT' }))
    expect(onFriendAction).toHaveBeenCalledWith('accept', '0xalice')
    await userEvent.click(screen.getByRole('button', { name: 'REJECT' }))
    expect(onFriendAction).toHaveBeenCalledWith('reject', '0xalice')
  })

  it('a blocked user shows Unblock (not Block / Add Friend)', async () => {
    const onFriendAction = vi.fn()
    renderCard({ relationship: 'blocked', onFriendAction })
    expect(screen.queryByRole('button', { name: /ADD FRIEND/i })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Block' })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Unblock' }))
    expect(onFriendAction).toHaveBeenCalledWith('unblock', '0xalice')
  })

  it('View Passport opens the passport for the user', async () => {
    const onViewProfile = vi.fn()
    renderCard({ onViewProfile })
    await userEvent.click(screen.getByRole('button', { name: 'View Passport' }))
    expect(onViewProfile).toHaveBeenCalledWith(expect.objectContaining({ address: '0xalice' }))
  })

  // Block closes the card and delegates to the parent, which owns the single confirm dialog
  // (the confirm-then-act flow is covered where App wires runBlock).
  it('Block closes the card and delegates to onBlock', async () => {
    const onBlock = vi.fn()
    const { onClose } = renderCard({ onBlock })
    await userEvent.click(screen.getByRole('button', { name: 'Block' }))
    expect(onBlock).toHaveBeenCalledWith(expect.objectContaining({ address: '0xalice' }))
    expect(onClose).toHaveBeenCalled()
  })
})
