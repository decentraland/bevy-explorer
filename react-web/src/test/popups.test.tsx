import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { PopupHost, openPopup, closeTopPopup, showConfirm, resetPopups } from '../design'

const pressEscape = (): void =>
  act(() => {
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  })

afterEach(resetPopups)

describe('popup stack', () => {
  it('closeTopPopup closes only the topmost popup (Escape is relayed here — see useEngineSession)', () => {
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>first</div>)
      openPopup(() => <div>second</div>)
    })
    expect(screen.getByText('first')).toBeTruthy()
    expect(screen.getByText('second')).toBeTruthy()

    act(() => closeTopPopup())
    expect(screen.queryByText('second')).toBeNull() // top closed
    expect(screen.getByText('first')).toBeTruthy() // the one below stays

    act(() => closeTopPopup())
    expect(screen.queryByText('first')).toBeNull()
  })

  it('closeTopPopup is a no-op when the stack is empty', () => {
    render(<PopupHost />)
    act(() => closeTopPopup()) // no throw
    expect(screen.queryByText('x')).toBeNull()
  })

  it('a backdrop popup closes on outside (backdrop) click; backdropClickCloses:false does not', () => {
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>plain</div>) // default { backdrop: true, backdropClickCloses: true }
    })
    const backdrop = document.querySelector('[class*="backdrop"]') as HTMLElement
    fireEvent.click(backdrop)
    expect(screen.queryByText('plain')).toBeNull()

    act(() => {
      openPopup(() => <div>locked</div>, { backdropClickCloses: false })
    })
    fireEvent.click(document.querySelector('[class*="backdrop"]') as HTMLElement)
    expect(screen.getByText('locked')).toBeTruthy() // stayed open
  })

  it('Escape closes the topmost popup, one layer at a time — the single central handler', () => {
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>first</div>)
      openPopup(() => <div>second</div>)
    })
    pressEscape()
    expect(screen.queryByText('second')).toBeNull()
    expect(screen.getByText('first')).toBeTruthy()
    pressEscape()
    expect(screen.queryByText('first')).toBeNull()
  })

  it('Escape passes through to the engine (window) when no popup is open, but is suppressed when one is', () => {
    render(<PopupHost />)
    const onWindow = vi.fn() // stands in for the mock/engine Cancel relay + Modal's own key handler
    window.addEventListener('keydown', onWindow)

    pressEscape() // nothing open → not swallowed, the engine still gets it
    expect(onWindow).toHaveBeenCalledTimes(1)

    act(() => {
      openPopup(() => <div>x</div>)
    })
    pressEscape() // popup open → PopupHost captures + stops, so no double-close downstream
    expect(onWindow).toHaveBeenCalledTimes(1) // unchanged: window never saw the second Escape
    expect(screen.queryByText('x')).toBeNull() // ...and the popup closed

    window.removeEventListener('keydown', onWindow)
  })

  it('runs the popup onClose exactly once, on every close path (Escape, handle, backdrop)', () => {
    render(<PopupHost />)

    const onClose = vi.fn()
    act(() => {
      openPopup(() => <div>a</div>, { onClose })
    })
    pressEscape()
    expect(onClose).toHaveBeenCalledTimes(1)

    const onClose2 = vi.fn()
    let close2!: () => void
    act(() => {
      close2 = openPopup(() => <div>b</div>, { onClose: onClose2 })
    })
    act(() => close2())
    act(() => close2()) // already gone → no-op
    expect(onClose2).toHaveBeenCalledTimes(1)

    const onClose3 = vi.fn()
    act(() => {
      openPopup(() => <div>c</div>, { onClose: onClose3 })
    })
    fireEvent.click(document.querySelector('[class*="backdrop"]') as HTMLElement)
    expect(onClose3).toHaveBeenCalledTimes(1)
  })

  it('a showConfirm dismissed with Escape resolves false (no hanging promise)', async () => {
    render(<PopupHost />)
    let confirmed!: Promise<boolean>
    act(() => {
      confirmed = showConfirm({ title: 'Sure?' })
    })
    expect(screen.getByText('Sure?')).toBeTruthy()
    pressEscape()
    expect(await confirmed).toBe(false)
    expect(screen.queryByText('Sure?')).toBeNull()
  })
})
