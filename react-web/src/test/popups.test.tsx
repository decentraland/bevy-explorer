import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { PopupHost, openPopup, closeTopPopup, showConfirm, resetPopups } from '../design'
import { CrashModal } from '../features/error/CrashModal'
import { openExitConfirm } from '../features/session/ExitConfirm'

// The popup layer has NO keyboard handling of its own: the cancel key flows to the engine
// like any other input and comes back as the 'Cancel' system action, which the session
// dispatcher turns into closeTopPopup() — see systemActionShortcuts.test.tsx for that path
// (and for the pre-world DOM fallback). These tests cover the stack semantics themselves.

const pressEscape = (): void =>
  act(() => {
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  })

afterEach(resetPopups)

describe('popup stack', () => {
  it('closeTopPopup closes only the topmost popup (the session Cancel handler calls this)', () => {
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

  it('a DOM Escape alone does not close popups — cancel is engine-resolved, and the key must reach the engine', () => {
    render(<PopupHost />)
    const onWindow = vi.fn() // stands in for boot.js's forward-to-canvas listener
    window.addEventListener('keydown', onWindow)
    act(() => {
      openPopup(() => <div>popup</div>)
    })
    pressEscape()
    expect(screen.getByText('popup')).toBeTruthy() // the layer itself never reacts to keys
    expect(onWindow).toHaveBeenCalledTimes(1) // ...and nothing swallowed the key on its way out
    window.removeEventListener('keydown', onWindow)
  })

  it('runs the popup onClose exactly once, on every close path (closeTopPopup, handle, backdrop)', () => {
    render(<PopupHost />)

    const onClose = vi.fn()
    act(() => {
      openPopup(() => <div>a</div>, { onClose })
    })
    act(() => closeTopPopup())
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

  it('a showConfirm dismissed via closeTopPopup (the Cancel path) resolves false (no hanging promise)', async () => {
    render(<PopupHost />)
    let confirmed!: Promise<boolean>
    act(() => {
      confirmed = showConfirm({ title: 'Sure?' })
    })
    expect(screen.getByText('Sure?')).toBeTruthy()
    act(() => closeTopPopup())
    expect(await confirmed).toBe(false)
    expect(screen.queryByText('Sure?')).toBeNull()
  })

  it('a crash modal freezes the popup layer: the popup underneath stays exactly as it was', () => {
    // (The input lock also gates the Cancel action in the session dispatcher — see
    // systemActionShortcuts.test.tsx — so nothing can close the popup invisibly.)
    const onClose = vi.fn()
    const { rerender } = render(
      <>
        <PopupHost />
      </>
    )
    act(() => {
      openPopup(() => <div>passport</div>, { onClose })
    })
    expect(screen.getByText('passport')).toBeInTheDocument()

    rerender(
      <>
        <PopupHost />
        <CrashModal error={{ message: 'boom', source: 'runtime' }} onReload={vi.fn()} onDismiss={vi.fn()} />
      </>
    )
    expect(screen.getByText('passport')).toBeInTheDocument()
    expect(onClose).not.toHaveBeenCalled()

    // Once the crash modal is dismissed, the popup closes normally again.
    rerender(<PopupHost />)
    act(() => closeTopPopup())
    expect(screen.queryByText('passport')).toBeNull()
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('openExitConfirm settles exactly one outcome: Leave never runs the stay contract, a dismiss stays once', async () => {
    render(<PopupHost />)

    const onStay = vi.fn()
    const onLeave = vi.fn()
    act(() => {
      openExitConfirm(onStay, onLeave)
    })
    fireEvent.click(screen.getByRole('button', { name: /Leave/i }))
    expect(onLeave).toHaveBeenCalledTimes(1)
    expect(onStay).not.toHaveBeenCalled() // close() fires onClose, but Leave already settled

    const onStay2 = vi.fn()
    const onLeave2 = vi.fn()
    act(() => {
      openExitConfirm(onStay2, onLeave2)
    })
    act(() => closeTopPopup())
    expect(onStay2).toHaveBeenCalledTimes(1)
    expect(onLeave2).not.toHaveBeenCalled()
  })
})
