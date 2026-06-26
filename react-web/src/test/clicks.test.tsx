import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../features/sidebar/Sidebar'
import { Chat } from '../features/chat/Chat'
import { FriendsPanel } from '../features/friends/FriendsPanel'
import { EmotesWheel } from '../features/emotes/EmotesWheel'
import type { EngineSession, FriendsState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

// CLICK coverage: render the real components and assert each click invokes the
// session method. Tier 1 proves every method posts the right wire message, so
// these complete the click → API-call chain through real DOM events.
describe('sidebar clicks', () => {
  const cases: [string, (s: EngineSession) => () => void][] = [
    ['Profile', (s) => s.profile.toggle],
    ['Notifications', (s) => s.notifications.toggle],
    ['Map', (s) => s.map.toggle],
    ['Communities', (s) => s.communities.toggle],
    ['Backpack', (s) => s.backpack.toggle],
    ['Settings', (s) => s.settings.toggle],
    ['Voice chat', (s) => s.mic.toggle],
    ['Emotes', (s) => s.emotes.toggle],
    ['Friends', (s) => s.friends.toggle],
    ['Chat', (s) => s.chat.toggle]
  ]

  it.each(cases)('clicking %s triggers its session action', async (name, pick) => {
    const s = fakeSession()
    render(<Sidebar session={s} />)
    await userEvent.click(screen.getByRole('button', { name }))
    expect(vi.mocked(pick(s))).toHaveBeenCalledTimes(1)
  })
})

describe('chat input', () => {
  it('typing a message and pressing Enter sends it', async () => {
    const s = fakeSession()
    render(<Chat chat={s.chat} />)
    await userEvent.type(screen.getByRole('textbox'), 'gm frens{Enter}')
    expect(vi.mocked(s.chat.send)).toHaveBeenCalledWith('gm frens')
  })

  it('does not send an empty message', async () => {
    const s = fakeSession()
    render(<Chat chat={s.chat} />)
    await userEvent.type(screen.getByRole('textbox'), '{Enter}')
    expect(vi.mocked(s.chat.send)).not.toHaveBeenCalled()
  })
})

describe('friends panel action clicks', () => {
  function renderPanel(over: Partial<FriendsState>): FriendsState {
    const friends: FriendsState = { ...fakeSession().friends, open: true, available: true, act: vi.fn(), ...over }
    render(<FriendsPanel friends={friends} />)
    return friends
  }

  it('accept / reject a received request', async () => {
    const friends = renderPanel({ received: [{ address: '0xr', name: 'R', id: 'r1' }] })
    await userEvent.click(screen.getByRole('button', { name: /Requests/ }))
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }))
    expect(vi.mocked(friends.act)).toHaveBeenCalledWith('accept', '0xr')
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }))
    expect(vi.mocked(friends.act)).toHaveBeenCalledWith('reject', '0xr')
  })

  it('cancel a sent request', async () => {
    const friends = renderPanel({ sent: [{ address: '0xs', name: 'S', id: 's1' }] })
    await userEvent.click(screen.getByRole('button', { name: /Requests/ }))
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(vi.mocked(friends.act)).toHaveBeenCalledWith('cancel', '0xs')
  })

  it('unblock a blocked user', async () => {
    const friends = renderPanel({ blocked: ['0xb'] })
    await userEvent.click(screen.getByRole('button', { name: /Blocked/ }))
    await userEvent.click(screen.getByRole('button', { name: 'Unblock' }))
    expect(vi.mocked(friends.act)).toHaveBeenCalledWith('unblock', '0xb')
  })
})

describe('emotes wheel clicks', () => {
  it('close button toggles the wheel', async () => {
    const s = fakeSession()
    s.emotes = { ...s.emotes, open: true, toggle: vi.fn() }
    render(<EmotesWheel emotes={s.emotes} />)
    await userEvent.click(screen.getByRole('button', { name: 'Close emotes' }))
    expect(vi.mocked(s.emotes.toggle)).toHaveBeenCalled()
  })
})
