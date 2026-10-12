import { describe, it, expect, vi } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CommunityModal } from '../features/communities/CommunityModal'
import type { Community } from '../engine/protocol'
import { enterAsGuest, renderSession } from './harness'

const community = (over: Partial<Community>): Community => ({
  id: 'c1',
  name: 'Builders',
  description: 'we build',
  membersCount: 12,
  role: 'none',
  ownerName: 'Owner',
  privacy: 'private',
  ...over
})

function renderModal(c: Community, error: string | null = null) {
  const props = { onJoin: vi.fn(), onRequestToJoin: vi.fn(), onCancelRequest: vi.fn() }
  render(
    <CommunityModal
      community={c}
      detail={null}
      error={error}
      {...props}
      onLeave={vi.fn()}
      onAddFriend={vi.fn()}
      onOpenChat={vi.fn()}
      onClose={vi.fn()}
    />
  )
  return props
}

// Unity CommunityCardController.RequestToJoinCommunity / CancelRequestToJoinCommunity.
describe('private community join requests', () => {
  it('Request to Join sends a join request, not a public join', async () => {
    const p = renderModal(community({}))
    await userEvent.click(screen.getByRole('button', { name: 'Request to Join' }))
    expect(p.onRequestToJoin).toHaveBeenCalledWith('c1')
    expect(p.onJoin).not.toHaveBeenCalled()
  })

  it('a pending request shows Requested, and clicking it cancels the request', async () => {
    const p = renderModal(community({ pendingRequestId: 'r9' }))
    await userEvent.click(screen.getByRole('button', { name: 'Cancel join request' }))
    expect(p.onCancelRequest).toHaveBeenCalledWith('c1', 'r9')
  })

  it('shows why the last action failed', () => {
    renderModal(community({}), 'Could not send your request. Try again.')
    expect(screen.getByRole('alert')).toHaveTextContent('Could not send your request. Try again.')
  })

  it('session sends the request and surfaces the bridge failure for that community', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().communities.requestToJoin('c1'))
    act(() => h.session().communities.cancelRequest('c1', 'r9'))
    expect(h.driver.sentOf('requestToJoinCommunity')).toEqual([{ kind: 'requestToJoinCommunity', id: 'c1' }])
    expect(h.driver.sentOf('cancelJoinRequest')).toEqual([{ kind: 'cancelJoinRequest', id: 'c1', requestId: 'r9' }])
    act(() => h.driver.emit({ kind: 'communityActionFailed', id: 'c1', action: 'requestToJoin', message: 'HTTP 400' }))
    expect(h.session().communities.error).toEqual({ id: 'c1', message: 'There was an error requesting to join community. Please try again.' })
    act(() => h.session().communities.join('c2'))
    expect(h.session().communities.error).toBeNull()
  })
})
