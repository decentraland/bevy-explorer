import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CommunitiesPage } from '../features/communities/CommunitiesPage'
import { CommunityModal } from '../features/communities/CommunityModal'
import { openCommunityCreateModal } from '../features/communities/CommunityCreateModal'
import { SessionProvider } from '../features/session/SessionContext'
import { PopupHost, closeTopPopup, resetPopups } from '../design'
import type { Community, CommunityDetailMessage } from '../engine/protocol'
import type { CommunitiesState } from '../features/session/useEngineSession'
import { fakeProfileState, fakeSession } from './harness'

afterEach(resetPopups) // the community modal now lives on the module-level popup stack

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
  // The community detail modal opens as a popup and reads live data via useSession(), so the page and
  // <PopupHost/> must share one SessionProvider (its `communities` is what the popup content reads).
  function renderPage(list: Community[]): CommunitiesState {
    const session = fakeSession()
    session.communities = { ...session.communities, open: true, list, join: vi.fn(), loadDetail: vi.fn() }
    render(
      <SessionProvider value={session}>
        <CommunitiesPage communities={session.communities} profile={fakeProfileState()} onNavigate={vi.fn()} />
        <PopupHost />
      </SessionProvider>
    )
    return session.communities
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

// The create modal holds typed details with nowhere to put them yet, so it takes the same close
// contract as the passport: the scrim refuses, the deliberate closes ask.
describe('community create modal guards what you typed', () => {
  const openWithAName = async (): Promise<void> => {
    render(<PopupHost />)
    act(() => {
      openCommunityCreateModal(true, vi.fn())
    })
    await userEvent.type(screen.getByLabelText(/community name/i), 'Builders')
  }

  it('ignores a backdrop click once something has been entered', async () => {
    await openWithAName()
    fireEvent.click(document.querySelector('[class*="backdrop"]') as HTMLElement)
    expect(screen.queryByText('Discard this community?')).toBeNull()
    expect((screen.getByLabelText(/community name/i) as HTMLInputElement).value).toBe('Builders')
  })

  it('asks on CANCEL, and keeps the details when the answer is no', async () => {
    await openWithAName()
    await userEvent.click(screen.getByRole('button', { name: 'CANCEL' }))
    await userEvent.click(screen.getByRole('button', { name: 'Keep editing' }))
    expect((screen.getByLabelText(/community name/i) as HTMLInputElement).value).toBe('Builders')
  })

  it('closes untouched, with nothing to lose', () => {
    render(<PopupHost />)
    act(() => {
      openCommunityCreateModal(true, vi.fn())
    })
    act(() => closeTopPopup())
    expect(screen.queryByRole('button', { name: 'CANCEL' })).toBeNull()
  })
})
