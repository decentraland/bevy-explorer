import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/react'
import { EmotesWheel } from '../features/emotes/EmotesWheel'
import type { EmotesState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function wheel(): EmotesState {
  return {
    ...fakeSession().emotes,
    open: true,
    list: [
      { slot: 1, urn: 'urn:wave', name: 'Wave' },
      { slot: 2, urn: 'urn:dance', name: 'Dance' }
    ],
    play: vi.fn()
  }
}

describe('emote wheel number keys', () => {
  it('the top-row digit plays the emote in that slot', () => {
    const emotes = wheel()
    render(<EmotesWheel emotes={emotes} />)
    fireEvent.keyDown(window, { key: '2', code: 'Digit2' })
    expect(emotes.play).toHaveBeenCalledWith('urn:dance')
  })

  it('a key that reaches the window twice plays the emote once', () => {
    const emotes = wheel()
    render(<EmotesWheel emotes={emotes} />)
    fireEvent.keyDown(window, { key: '1', code: 'Digit1' })
    fireEvent.keyDown(window, { key: '1', code: 'Digit1' })
    expect(emotes.play).toHaveBeenCalledTimes(1)
  })
})
