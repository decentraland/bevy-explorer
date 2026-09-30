import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfileCardPresentation } from '../features/chat/ProfileCardPresentation'
import { PopupHost, resetPopups } from '../design'

// The popup store is a module singleton — clear it between tests so open dialogs don't leak.
afterEach(resetPopups)

// DOMAIN: profile-card — the shared popover (chat / friends / world avatar click). Covers the
// action set ported from bevy-ui-scene's profile-menu: relationship-driven friend CTA (Add /
// Accept+Reject / Requested / Unblock), View Passport, Mention, Block. (Report was removed until a
// moderation endpoint exists.)
const ALICE = { address: '0xalice', name: 'Alice' }

function renderCard(props: Partial<React.ComponentProps<typeof ProfileCardPresentation>> = {}): { onClose: () => void } {
  const onClose = vi.fn()
  // PopupHost renders the imperative confirm (Block) opened via showConfirm.
  render(
    <>
      <ProfileCardPresentation user={ALICE} x={20} y={20} me={{ address: '0xme' }} onClose={onClose} {...props} />
      <PopupHost />
    </>
  )
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

  // Block closes the card and opens a confirm (via the popup layer); confirming fires the friend action.
  it('Block closes the card and opens a confirm that fires onFriendAction', async () => {
    const onFriendAction = vi.fn()
    const { onClose } = renderCard({ onFriendAction })
    await userEvent.click(screen.getByRole('button', { name: 'Block' }))
    expect(onClose).toHaveBeenCalled()
    const confirm = screen.getByText('Block Alice?').closest('[role="dialog"]') as HTMLElement
    expect(onFriendAction).not.toHaveBeenCalled()
    await userEvent.click(within(confirm).getByRole('button', { name: 'Block' }))
    expect(onFriendAction).toHaveBeenCalledWith('block', '0xalice')
  })
})
