import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Chat } from '../features/chat/Chat'
import type { ChatLine, ChatState } from '../features/session/useEngineSession'
import type { NearbyMember } from '../engine/protocol'
import { fakeSession } from './harness'

const line = (message: string, sender = '0xsender'): ChatLine => ({
  sender,
  message,
  channel: 'Nearby',
  id: 1,
  ts: 1_700_000_000_000
})

function renderChat(
  opts: {
    messages?: ChatLine[]
    members?: NearbyMember[]
    onAddFriend?: (a: string) => void
    onTeleport?: (x: number, y: number) => void
    me?: { address?: string; name?: string } | null
  } = {}
) {
  const chat: ChatState = {
    ...fakeSession().chat,
    open: true,
    send: vi.fn(),
    toggle: vi.fn(),
    messages: opts.messages ?? [],
    members: opts.members ?? []
  }
  const { container } = render(
    <Chat chat={chat} me={opts.me} onAddFriend={opts.onAddFriend} onTeleport={opts.onTeleport} />
  )
  return { chat, container }
}

describe('chat rich messages', () => {
  it('renders a URL as an external link', () => {
    renderChat({ messages: [line('visit https://decentraland.org now')] })
    const link = screen.getByRole('link', { name: 'https://decentraland.org' })
    expect(link).toHaveAttribute('href', 'https://decentraland.org')
    expect(link).toHaveAttribute('target', '_blank')
  })

  it('a location coord link teleports', async () => {
    const onTeleport = vi.fn()
    renderChat({ messages: [line('meet at 10,-5')], onTeleport })
    await userEvent.click(screen.getByRole('button', { name: '10,-5' }))
    expect(onTeleport).toHaveBeenCalledWith(10, -5)
  })

  it('an @mention opens the profile viewer and can add a friend', async () => {
    const onAddFriend = vi.fn()
    renderChat({
      messages: [line('yo @Alice')],
      members: [{ address: '0xalice', name: 'Alice', picture: 'p.png' }],
      onAddFriend
    })
    await userEvent.click(screen.getByRole('button', { name: '@Alice' }))
    expect(screen.getByRole('dialog', { name: 'Profile' })).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Add Friend' }))
    expect(onAddFriend).toHaveBeenCalledWith('0xalice')
  })

  it('clicking a sender name opens their profile viewer', async () => {
    renderChat({ messages: [line('gm', '0xbob')], members: [{ address: '0xbob', name: 'Bob' }] })
    await userEvent.click(screen.getByRole('button', { name: 'View Bob' })) // avatar button
    expect(screen.getByRole('dialog', { name: 'Profile' })).toBeInTheDocument()
  })

  it('@mention autocomplete inserts the picked name', async () => {
    renderChat({ members: [{ address: '0xalice', name: 'Alice' }] })
    const input = screen.getByRole('textbox')
    await userEvent.type(input, 'hey @Al')
    await userEvent.click(screen.getByText('Alice')) // the suggestion row
    expect((input as HTMLInputElement).value).toContain('@Alice ')
  })

  it('does not linkify a bare name with no roster match', () => {
    renderChat({ messages: [line('hi @nobody there')] })
    // No resolvable mention → plain text, not a button.
    expect(screen.queryByRole('button', { name: '@nobody' })).toBeNull()
    // location/url regexes shouldn't fire either
    expect(screen.queryByRole('link')).toBeNull()
  })
})
