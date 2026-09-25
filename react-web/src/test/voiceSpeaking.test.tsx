import { describe, it, expect } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import { MemberRow } from '../features/chat/Chat'
import { enterAsGuest, renderSession } from './harness'

const ADDR = '0x5854cce95d5e25817b41f4c41f06b695a83bc495'

// Unity NametagElement --speaking / nearby voice: who is talking right now (engine getVoiceStream).
describe('voice speaking indicators', () => {
  it('tracks who is speaking while the engine reports them active', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().chat.speaking.has(ADDR)).toBe(false)
    act(() => h.driver.emit({ kind: 'voiceActivity', address: ADDR.toUpperCase().replace('0X', '0x'), active: true }))
    expect(h.session().chat.speaking.has(ADDR)).toBe(true)
    act(() => h.driver.emit({ kind: 'voiceActivity', address: ADDR, active: false }))
    expect(h.session().chat.speaking.has(ADDR)).toBe(false)
  })

  it('the nearby list shows Speaking with the voice bars', () => {
    render(<MemberRow member={{ address: ADDR, name: 'Mojito' }} speaking />)
    expect(screen.getByText('Speaking')).toBeInTheDocument()
    expect(document.querySelector('[data-speaking-bars]')).not.toBeNull()
  })

  it('a quiet member shows Online and no bars', () => {
    render(<MemberRow member={{ address: ADDR, name: 'Mojito' }} />)
    expect(screen.getByText('Online')).toBeInTheDocument()
    expect(document.querySelector('[data-speaking-bars]')).toBeNull()
  })
})
