import { describe, it, expect, vi, afterEach } from 'vitest'
import { render } from '@testing-library/react'
import { Chat } from '../features/chat/Chat'
import type { ChatLine, ChatState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

afterEach(() => vi.restoreAllMocks())

const lines = (n: number): ChatLine[] =>
  Array.from({ length: n }, (_, i) => ({ sender: `0x${i % 5}`, message: `hi ${i}`, channel: 'Nearby', id: i, ts: 1_700_000_000_000 + i }))

describe('Chat rendering', () => {
  it('a new message renders only its own bubble, not the whole history', () => {
    const me = { address: '0xme', name: 'me' }
    const members: ChatState['members'] = [] // session state: same array until the roster changes
    const chat = (messages: ChatLine[]): ChatState => ({ ...fakeSession().chat, open: true, messages, members })
    const history = lines(20)
    // App passes fresh arrows on every HUD render.
    const r = render(<Chat chat={chat(history)} me={me} onTeleport={() => {}} onVisitWorld={() => {}} />)
    const bubbleRenders = vi.spyOn(Date.prototype, 'toLocaleTimeString')
    r.rerender(<Chat chat={chat([...history, ...lines(21).slice(20)])} me={me} onTeleport={() => {}} onVisitWorld={() => {}} />)
    expect(bubbleRenders).toHaveBeenCalledTimes(1)
  })
})
