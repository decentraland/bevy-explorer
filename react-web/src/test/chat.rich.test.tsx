import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Chat } from '../features/chat/Chat'
import { openProfileCard } from '../features/profileCard/ProfileCard'
import type { ChatLine, ChatState } from '../features/session/useEngineSession'
import type { NearbyMember } from '../engine/protocol'
import { fakeSession } from './harness'

// Chat opens the shared profile card via openProfileCard (the card itself is covered by the container
// + presentational tests). Stub it so we can assert the trigger + the resolved address.
vi.mock('../features/profileCard/ProfileCard', () => ({ openProfileCard: vi.fn() }))
afterEach(() => vi.mocked(openProfileCard).mockClear())

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
    onTeleport?: (x: number, y: number) => void
    me?: { address?: string; name?: string } | null
    chatOver?: Partial<ChatState>
  } = {}
): { chat: ChatState; container: HTMLElement } {
  const chat: ChatState = {
    ...fakeSession().chat,
    open: true,
    send: vi.fn(),
    toggle: vi.fn(),
    messages: opts.messages ?? [],
    members: opts.members ?? [],
    ...opts.chatOver
  }
  const { container } = render(<Chat chat={chat} me={opts.me} onTeleport={opts.onTeleport} />)
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

  it('clicking a sender opens the shared profile card', async () => {
    renderChat({ messages: [line('gm', '0xbob')], members: [{ address: '0xbob', name: 'Bob' }] })
    await userEvent.click(screen.getByRole('button', { name: 'View Bob' })) // avatar button
    expect(openProfileCard).toHaveBeenCalledWith('0xbob', expect.any(Number), expect.any(Number))
  })

  it('right-clicking a sender also opens the card', () => {
    renderChat({ messages: [line('gm', '0xbob')], members: [{ address: '0xbob', name: 'Bob' }] })
    fireEvent.contextMenu(screen.getByRole('button', { name: 'View Bob' }))
    expect(openProfileCard).toHaveBeenCalledWith('0xbob', expect.any(Number), expect.any(Number))
  })

  it('clicking an @mention opens the card for that user', async () => {
    renderChat({ messages: [line('yo @Alice')], members: [{ address: '0xalice', name: 'Alice', picture: 'p.png' }] })
    await userEvent.click(screen.getByRole('button', { name: '@Alice' }))
    expect(openProfileCard).toHaveBeenCalledWith('0xalice', expect.any(Number), expect.any(Number))
  })

  it('a mention queued from another surface (the card) drops @name into the draft', async () => {
    // session.chat.mention → pendingMention → this effect → insertMention (the receiving end).
    renderChat({ chatOver: { pendingMention: 'Alice', consumeMention: vi.fn() } })
    await waitFor(() => expect((screen.getByRole('textbox') as HTMLInputElement).value).toContain('@Alice'))
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
