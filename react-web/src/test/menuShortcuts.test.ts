import { describe, it, expect, afterEach } from 'vitest'
import { renderHook } from '@testing-library/react'
import { fakeSession } from './harness'
import { useMenuShortcuts } from '../lib/useMenuShortcuts'

function press(key: string, init: KeyboardEventInit = {}, target?: EventTarget): void {
  const ev = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  ;(target ?? window).dispatchEvent(ev)
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('useMenuShortcuts', () => {
  it('toggles the matching panel for each shortcut key', () => {
    const session = fakeSession()
    renderHook(() => useMenuShortcuts(session))
    press('m')
    press('o')
    press('I') // case-insensitive
    press('g')
    press('p')
    expect(session.map.toggle).toHaveBeenCalledTimes(1)
    expect(session.communities.toggle).toHaveBeenCalledTimes(1)
    expect(session.backpack.toggle).toHaveBeenCalledTimes(1)
    expect(session.gallery.toggle).toHaveBeenCalledTimes(1)
    expect(session.settings.toggle).toHaveBeenCalledTimes(1)
  })

  it('ignores modified chords (so Cmd+P prints, etc.)', () => {
    const session = fakeSession()
    renderHook(() => useMenuShortcuts(session))
    press('p', { metaKey: true })
    press('m', { ctrlKey: true })
    expect(session.settings.toggle).not.toHaveBeenCalled()
    expect(session.map.toggle).not.toHaveBeenCalled()
  })

  it('ignores keystrokes aimed at text inputs', () => {
    const session = fakeSession()
    renderHook(() => useMenuShortcuts(session))
    const input = document.createElement('input')
    document.body.appendChild(input)
    press('m', {}, input)
    expect(session.map.toggle).not.toHaveBeenCalled()
  })

  it('does nothing outside the world phase', () => {
    const session = { ...fakeSession(), phase: 'login' as const }
    renderHook(() => useMenuShortcuts(session))
    press('m')
    expect(session.map.toggle).not.toHaveBeenCalled()
  })

  it('quick emotes: a number plays that slot, but only while the wheel is open', () => {
    const emote = { slot: 3, urn: 'urn:emote:three', name: 'Three' }

    // Wheel closed → a number does nothing.
    const closed = fakeSession()
    closed.emotes.list = [emote]
    const { unmount } = renderHook(() => useMenuShortcuts(closed))
    press('3')
    expect(closed.emotes.play).not.toHaveBeenCalled()
    unmount()

    // Wheel open → the number plays that slot's emote; an empty slot (7) is a no-op.
    const open = fakeSession()
    open.emotes = { ...open.emotes, open: true, list: [emote] }
    renderHook(() => useMenuShortcuts(open))
    press('3')
    press('7')
    expect(open.emotes.play).toHaveBeenCalledTimes(1)
    expect(open.emotes.play).toHaveBeenCalledWith('urn:emote:three')
  })
})
