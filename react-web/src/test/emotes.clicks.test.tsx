import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { EmotesWheel } from '../features/emotes/EmotesWheel'
import { fakeSession } from './harness'

describe('emotes wheel slot play', () => {
  it('clicking a filled slot plays that emote', () => {
    const s = fakeSession()
    s.emotes = { ...s.emotes, open: true, list: [{ slot: 1, urn: 'urn:wave', name: 'Wave' }], play: vi.fn() }
    render(<EmotesWheel emotes={s.emotes} />)
    fireEvent.click(screen.getByText('1')) // slot numbered 1 (bubbles to the slot's onClick)
    expect(vi.mocked(s.emotes.play)).toHaveBeenCalledWith('urn:wave')
  })

  it('empty slots do nothing', () => {
    const s = fakeSession()
    s.emotes = { ...s.emotes, open: true, list: [], play: vi.fn() }
    render(<EmotesWheel emotes={s.emotes} />)
    fireEvent.click(screen.getByText('3'))
    expect(vi.mocked(s.emotes.play)).not.toHaveBeenCalled()
  })

  it('"Customise [E]" — click and the E key both open the backpack on the Emotes tab', () => {
    const s = fakeSession()
    s.emotes = { ...s.emotes, open: true }
    const onCustomise = vi.fn()
    render(<EmotesWheel emotes={s.emotes} onCustomise={onCustomise} />)
    fireEvent.click(screen.getByRole('button', { name: /Customise/i }))
    expect(onCustomise).toHaveBeenCalledTimes(1)
    fireEvent.keyDown(window, { key: 'e' })
    expect(onCustomise).toHaveBeenCalledTimes(2)
  })
})
