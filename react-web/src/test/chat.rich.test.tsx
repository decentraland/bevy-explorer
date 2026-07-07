import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Chat } from '../features/chat/Chat'
import { PopupHost, resetPopups } from '../design'
import type { ChatLine, ChatState } from '../features/session/useEngineSession'
import type { FriendAction, NearbyMember } from '../engine/protocol'
import { fakeSession } from './harness'

// The popup store is a module singleton — clear it between tests so open dialogs don't leak.
afterEach(resetPopups)

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
    onFriendAction?: (op: FriendAction, a: string) => void
    onViewProfile?: (u: { address: string; name: string }) => void
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
    <>
      <Chat chat={chat} me={opts.me} onFriendAction={opts.onFriendAction} onViewProfile={opts.onViewProfile} onTeleport={opts.onTeleport} />
      <PopupHost />
    </>
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

  it('an @mention opens the profile card with the supported actions', async () => {
    const onFriendAction = vi.fn()
    renderChat({
      messages: [line('yo @Alice')],
      members: [{ address: '0xalice', name: 'Alice', picture: 'p.png' }],
      onFriendAction
    })
    await userEvent.click(screen.getByRole('button', { name: '@Alice' }))
    const dialog = screen.getByRole('dialog', { name: 'Profile' })
    expect(dialog).toBeInTheDocument()
    // Only the actions wired here — no View Passport (that callback wasn't passed).
    expect(screen.queryByRole('button', { name: /View Passport/i })).toBeNull()

    await userEvent.click(screen.getByRole('button', { name: /ADD FRIEND/i }))
    expect(onFriendAction).toHaveBeenCalledWith('request', '0xalice')
  })

  it('the profile card Block opens a confirm that fires onFriendAction', async () => {
    const onFriendAction = vi.fn()
    renderChat({ messages: [line('yo @Alice')], members: [{ address: '0xalice', name: 'Alice' }], onFriendAction })
    await userEvent.click(screen.getByRole('button', { name: '@Alice' }))
    await userEvent.click(screen.getByRole('button', { name: 'Block' }))
    const confirm = screen.getByText('Block Alice?').closest('[role="dialog"]') as HTMLElement
    await userEvent.click(within(confirm).getByRole('button', { name: 'Block' }))
    expect(onFriendAction).toHaveBeenCalledWith('block', '0xalice')
  })

  it('View Passport from the menu opens the passport for that user', async () => {
    const onViewProfile = vi.fn()
    renderChat({ messages: [line('yo @Alice')], members: [{ address: '0xalice', name: 'Alice' }], onViewProfile })
    await userEvent.click(screen.getByRole('button', { name: '@Alice' }))
    await userEvent.click(screen.getByRole('button', { name: 'View Passport' }))
    expect(onViewProfile).toHaveBeenCalledWith(expect.objectContaining({ address: '0xalice' }))
  })

  it('Mention from the menu drops @name into the draft', async () => {
    renderChat({ messages: [line('yo @Alice')], members: [{ address: '0xalice', name: 'Alice' }] })
    await userEvent.click(screen.getByRole('button', { name: '@Alice' }))
    await userEvent.click(screen.getByRole('button', { name: 'Mention' }))
    expect((screen.getByRole('textbox') as HTMLInputElement).value).toContain('@Alice')
  })

  it('clicking a sender name opens their profile viewer', async () => {
    renderChat({ messages: [line('gm', '0xbob')], members: [{ address: '0xbob', name: 'Bob' }] })
    await userEvent.click(screen.getByRole('button', { name: 'View Bob' })) // avatar button
    expect(screen.getByRole('dialog', { name: 'Profile' })).toBeInTheDocument()
  })

  it('right-clicking a sender also opens the profile menu', () => {
    renderChat({ messages: [line('gm', '0xbob')], members: [{ address: '0xbob', name: 'Bob' }] })
    fireEvent.contextMenu(screen.getByRole('button', { name: 'View Bob' }))
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
