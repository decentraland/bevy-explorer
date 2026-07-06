import { describe, it, expect, vi } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfileCard } from '../features/chat/ProfileCard'

// DOMAIN: profile-card — the shared popover (chat / friends / world avatar click). Covers the
// action set ported from bevy-ui-scene's profile-menu: relationship-driven friend CTA (Add /
// Accept+Reject / Requested / Unblock), View Passport, Mention, Block, Report. ("Invite to
// Community" is parked until the communities feature — see backlog.md.)
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

  it('Report asks for confirmation before firing', async () => {
    const onReport = vi.fn()
    renderCard({ onReport })
    await userEvent.click(screen.getByRole('button', { name: 'Report' }))
    // A confirm dialog appears; onReport only fires on confirm.
    const confirm = screen.getByText('Report Alice?').closest('[role="dialog"]') as HTMLElement
    expect(onReport).not.toHaveBeenCalled()
    await userEvent.click(within(confirm).getByRole('button', { name: 'Report' }))
    expect(onReport).toHaveBeenCalledWith(expect.objectContaining({ address: '0xalice' }))
  })

  it('Block asks for confirmation before firing', async () => {
    const onFriendAction = vi.fn()
    renderCard({ onFriendAction })
    await userEvent.click(screen.getByRole('button', { name: 'Block' }))
    // A confirm dialog appears; onFriendAction only fires on confirm.
    const confirm = screen.getByText('Block Alice?').closest('[role="dialog"]') as HTMLElement
    expect(onFriendAction).not.toHaveBeenCalled()
    await userEvent.click(within(confirm).getByRole('button', { name: 'Block' }))
    expect(onFriendAction).toHaveBeenCalledWith('block', '0xalice')
  })
})
