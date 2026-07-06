import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CommunitiesPage } from '../features/communities/CommunitiesPage'
import { CommunityModal } from '../features/communities/CommunityModal'
import type { Community, CommunityDetailMessage } from '../engine/protocol'
import type { CommunitiesState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

const community = (over: Partial<Community>): Community => ({
  id: 'c1',
  name: 'Builders',
  description: 'we build',
  membersCount: 12,
  role: 'none',
  ownerName: 'Owner',
  privacy: 'public',
  ...over
})

describe('communities page clicks', () => {
  function renderPage(list: Community[]): CommunitiesState {
    const communities: CommunitiesState = {
      ...fakeSession().communities,
      open: true,
      list,
      join: vi.fn(),
      loadDetail: vi.fn()
    }
    render(
      <CommunitiesPage
        communities={communities}
        profile={{ data: null, open: false, toggle: vi.fn() }}
        onNavigate={vi.fn()}
        onAddFriend={vi.fn()}
        onOpenChat={vi.fn()}
      />
    )
    return communities
  }

  it('Join on a browse card joins that community', async () => {
    const communities = renderPage([community({ id: 'c1', role: 'none' })])
    await userEvent.click(screen.getByRole('button', { name: 'Join' }))
    expect(vi.mocked(communities.join)).toHaveBeenCalledWith('c1')
  })

  it('View on a joined community opens its modal and loads detail', async () => {
    const communities = renderPage([community({ id: 'c1', role: 'member' })])
    await userEvent.click(screen.getByRole('button', { name: 'View' }))
    expect(vi.mocked(communities.loadDetail)).toHaveBeenCalledWith('c1')
    expect(screen.getByRole('heading', { name: 'Builders' })).toBeInTheDocument()
  })
})

describe('community modal clicks', () => {
  const detail = (over: Partial<CommunityDetailMessage> = {}): CommunityDetailMessage => ({
    kind: 'communityDetail',
    id: 'c1',
    members: [],
    posts: [],
    places: [],
    events: [],
    photos: [],
    ...over
  })

  function renderModal(c: Community, d: CommunityDetailMessage | null = detail()) {
    const spies = { onJoin: vi.fn(), onLeave: vi.fn(), onAddFriend: vi.fn(), onOpenChat: vi.fn(), onClose: vi.fn() }
    render(<CommunityModal community={c} detail={d} {...spies} />)
    return spies
  }

  it('Join (non-member) joins', async () => {
    const s = renderModal(community({ role: 'none', privacy: 'public' }))
    await userEvent.click(screen.getByRole('button', { name: 'Join' }))
    expect(s.onJoin).toHaveBeenCalledWith('c1')
  })

  it('member: open chat + leave via the more menu', async () => {
    const s = renderModal(community({ role: 'member' }))
    await userEvent.click(screen.getByRole('button', { name: 'Open chat' }))
    expect(s.onOpenChat).toHaveBeenCalledTimes(1)
    await userEvent.click(screen.getByRole('button', { name: 'More' }))
    await userEvent.click(screen.getByRole('button', { name: 'Leave community' }))
    expect(s.onLeave).toHaveBeenCalledWith('c1')
    expect(s.onClose).toHaveBeenCalledTimes(1)
  })

  it('Members tab: ADD FRIEND sends a request', async () => {
    const s = renderModal(
      community({ role: 'member' }),
      detail({ members: [{ address: '0xm', name: 'M', role: 'member', isFriend: false }] })
    )
    await userEvent.click(screen.getByRole('button', { name: 'MEMBERS' }))
    await userEvent.click(screen.getByRole('button', { name: /ADD FRIEND/i }))
    expect(s.onAddFriend).toHaveBeenCalledWith('0xm')
  })

  it('close button closes the modal', async () => {
    const s = renderModal(community({}))
    await userEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(s.onClose).toHaveBeenCalledTimes(1)
  })
})
