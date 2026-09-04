import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Chat } from '../features/chat/Chat'
import { EmojiPicker } from '../features/chat/EmojiPicker'
import { EMOJI_GROUPS } from '../features/chat/emojiData'
import type { ChatState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function chatState(over: Partial<ChatState> = {}): ChatState {
  return { ...fakeSession().chat, open: true, send: vi.fn(), toggle: vi.fn(), members: [], ...over }
}

// Activate the chat (full chrome) by hovering its root — avoids the focus-then-blur
// race that would unmount the chrome mid-click.
function renderActiveChat(over: Partial<ChatState> = {}): { container: HTMLElement; chat: ChatState } {
  const chat = chatState(over)
  const { container } = render(<Chat chat={chat} />)
  fireEvent.mouseEnter(container.firstChild as Element)
  return { container, chat }
}

describe('chat chrome clicks', () => {
  it('the emoji button opens the picker', async () => {
    renderActiveChat()
    await userEvent.click(screen.getByRole('button', { name: 'Emoji' }))
    expect(screen.getByPlaceholderText('Search emoji')).toBeInTheDocument()
  })

  it('the members button reveals the nearby overlay', async () => {
    renderActiveChat({ members: [{ address: '0x1', name: 'Alice' }] })
    await userEvent.click(screen.getByRole('button', { name: '1 nearby' }))
    expect(screen.getByText('Alice')).toBeInTheDocument()
  })

  it('the nav close button toggles the chat', async () => {
    const { chat } = renderActiveChat()
    await userEvent.click(screen.getByRole('button', { name: 'Close chat' }))
    expect(vi.mocked(chat.toggle)).toHaveBeenCalledTimes(1)
  })
})

describe('emoji picker', () => {
  it('picking an emoji calls onPick with the glyph', async () => {
    const onPick = vi.fn()
    render(<EmojiPicker onPick={onPick} onClose={vi.fn()} />)
    const first = EMOJI_GROUPS[0].emojis[0]
    await userEvent.click(screen.getAllByTitle(first.expression)[0])
    expect(onPick).toHaveBeenCalledWith(first.emoji)
  })

  it('close button fires onClose', async () => {
    const onClose = vi.fn()
    render(<EmojiPicker onPick={vi.fn()} onClose={onClose} />)
    await userEvent.click(screen.getByTitle('Close'))
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
