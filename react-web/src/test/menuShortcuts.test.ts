import { describe, it, expect, afterEach, vi } from 'vitest'
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

  // The engine shares this document and winit reads keys off the canvas — downstream of our
  // window-capture listener. Enter has to reach it (that's what fires the "Chat" system action the
  // bridge turns into a camera-look release); the letter shortcuts must not, or the engine acts on
  // them too. Stopping Enter here left chat focused with the mouse still spinning the camera.
  it('lets Enter through to the engine canvas but swallows the letter shortcuts', () => {
    const session = fakeSession()
    renderHook(() => useMenuShortcuts(session))
    const canvas = document.createElement('canvas')
    document.body.appendChild(canvas)
    const engine = vi.fn()
    canvas.addEventListener('keydown', engine)

    press('Enter', {}, canvas)
    expect(session.chat.requestFocus).toHaveBeenCalledTimes(1)
    expect(engine).toHaveBeenCalledTimes(1)

    press('m', {}, canvas)
    expect(session.map.toggle).toHaveBeenCalledTimes(1)
    expect(engine).toHaveBeenCalledTimes(1) // unchanged: 'm' stopped at window capture
  })

  it('does nothing outside the world phase', () => {
    const session = { ...fakeSession(), phase: 'login' as const }
    renderHook(() => useMenuShortcuts(session))
    press('m')
    expect(session.map.toggle).not.toHaveBeenCalled()
  })
})
