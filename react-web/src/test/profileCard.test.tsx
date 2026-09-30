import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfileCardPresentation } from '../features/chat/ProfileCardPresentation'
import { PopupHost, resetPopups } from '../design'

// The popup store is a module singleton — clear it between tests so open dialogs don't leak.
afterEach(resetPopups)

// DOMAIN: profile-card — the shared popover (chat / friends / world avatar click): the
// relationship-driven friendship button and the menu items.
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
  it('an incoming request offers a single Accept Friend', async () => {
    const onAcceptRequest = vi.fn()
    renderCard({ relationship: 'incoming', onAcceptRequest, onAddFriend: vi.fn() })
    expect(screen.queryByRole('button', { name: /Add Friend/i })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Accept Friend' }))
    expect(onAcceptRequest).toHaveBeenCalledWith(expect.objectContaining({ address: '0xalice' }))
  })

  it('a blocked user gets no friendship button and no Block', () => {
    renderCard({ relationship: 'blocked', onAddFriend: vi.fn(), onBlock: vi.fn() })
    expect(screen.queryByRole('button', { name: /Add Friend/i })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Block' })).toBeNull()
  })

  it('View Profile opens the passport for the user', async () => {
    const onViewProfile = vi.fn()
    renderCard({ onViewProfile })
    await userEvent.click(screen.getByRole('button', { name: 'View Profile' }))
    expect(onViewProfile).toHaveBeenCalledWith(expect.objectContaining({ address: '0xalice' }))
  })

  it('Block closes the card and hands the user to onBlock', async () => {
    const onBlock = vi.fn()
    const { onClose } = renderCard({ onBlock })
    await userEvent.click(screen.getByRole('button', { name: 'Block' }))
    expect(onClose).toHaveBeenCalled()
    expect(onBlock).toHaveBeenCalledWith(expect.objectContaining({ address: '0xalice' }))
  })
})
