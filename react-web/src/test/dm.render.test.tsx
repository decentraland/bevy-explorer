import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Chat } from '../features/chat/Chat'
import type { ChatState, Conversation } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

// DOMAIN: direct messages in the chat panel — the conversation rail, the DM title bar and its
// menu, and the input replaced by the reason while the partner cannot be messaged.
const BOB = '0x2b6d2d8cd70b5e9548e87f871d4642e0d6387cd7'
const convo = (over: Partial<Conversation> = {}): Conversation => ({ address: BOB, unread: 0, state: 'connected', online: true, historyLoaded: true, ...over })
const chatWith = (over: Partial<ChatState>): ChatState => ({ ...fakeSession().chat, open: true, ...over })

describe('DM chat panel', () => {
  it('shows the rail once a DM tab exists, with Nearby first and the tab selectable', async () => {
    const chat = chatWith({ conversations: [convo({ unread: 3 })], select: vi.fn() })
    render(<Chat chat={chat} />)
    const rail = screen.getByRole('navigation', { name: 'Conversations' })
    expect(rail).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Nearby chat' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByText('3')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: /^Chat with/ }))
    expect(vi.mocked(chat.select)).toHaveBeenCalledWith(BOB)
  })

  it('no rail without DM tabs', () => {
    render(<Chat chat={chatWith({})} />)
    expect(screen.queryByRole('navigation', { name: 'Conversations' })).toBeNull()
  })

  it('a DM channel shows the partner title bar, and its menu deletes history or closes the tab', async () => {
    const chat = chatWith({ channel: BOB, conversations: [convo()], deleteHistory: vi.fn(), closeConversation: vi.fn() })
    render(<Chat chat={chat} />)
    expect(screen.getByText('Online')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /nearby$/ })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Conversation options' }))
    await userEvent.click(screen.getByText('Delete chat history'))
    expect(vi.mocked(chat.deleteHistory)).toHaveBeenCalledWith(BOB)
    await userEvent.click(screen.getByRole('button', { name: 'Conversation options' }))
    await userEvent.click(screen.getByText('Close conversation'))
    expect(vi.mocked(chat.closeConversation)).toHaveBeenCalledWith(BOB)
  })

  it('an unreachable partner replaces the input with the reason', () => {
    render(<Chat chat={chatWith({ channel: BOB, conversations: [convo({ state: 'privateMessagesBlocked', online: true })] })} />)
    expect(screen.queryByRole('textbox')).toBeNull()
    expect(screen.getByRole('status')).toHaveTextContent('only accepts DMs from friends')
  })

  it("the own-setting reason links to settings", async () => {
    const onOpenSettings = vi.fn()
    render(<Chat chat={chatWith({ channel: BOB, conversations: [convo({ state: 'privateMessagesBlockedByOwnUser' })] })} onOpenSettings={onOpenSettings} />)
    await userEvent.click(screen.getByRole('button', { name: 'DM settings' }))
    expect(onOpenSettings).toHaveBeenCalled()
  })

  it('not being in the chat room shows that, whoever the partner is', () => {
    render(<Chat chat={chatWith({ channel: BOB, conversations: [convo({ state: 'notConnected', online: false })] })} />)
    expect(screen.queryByRole('textbox')).toBeNull()
    expect(screen.getByRole('status')).toHaveTextContent('You are not connected to chat.')
  })

  it('a connected partner keeps the input', () => {
    render(<Chat chat={chatWith({ channel: BOB, conversations: [convo()] })} />)
    expect(screen.getByRole('textbox')).toBeInTheDocument()
  })
})
