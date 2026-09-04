import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CrashModal } from '../features/error/CrashModal'
import { openRealmError } from '../features/error/RealmErrorModal'
import { ErrorBoundary } from '../features/error/ErrorBoundary'
import { isInputLocked } from '../lib/inputLock'
import { PopupHost, closeTopPopup, resetPopups } from '../design'

afterEach(resetPopups)

describe('CrashModal', () => {
  it('shows the crash message + Reload, calls onReload', async () => {
    const onReload = vi.fn()
    render(<CrashModal error={{ message: "can't init wasm queue", source: 'launch' }} onReload={onReload} />)
    expect(screen.getByText(/can't init wasm queue/i)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: /Reload/i }))
    expect(onReload).toHaveBeenCalledTimes(1)
  })

  it('shows Dismiss only when onDismiss is provided (runtime crash)', () => {
    const { rerender } = render(<CrashModal error={{ message: 'boom', source: 'launch' }} onReload={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /Dismiss/i })).toBeNull()
    rerender(<CrashModal error={{ message: 'boom', source: 'runtime' }} onReload={vi.fn()} onDismiss={vi.fn()} />)
    expect(screen.getByRole('button', { name: /Dismiss/i })).toBeInTheDocument()
  })

  it('holds the HUD input lock while mounted, releases it on unmount — the gate every global hotkey checks', () => {
    expect(isInputLocked()).toBe(false)
    const { unmount } = render(<CrashModal error={{ message: 'boom', source: 'launch' }} onReload={vi.fn()} />)
    expect(isInputLocked()).toBe(true)
    unmount()
    expect(isInputLocked()).toBe(false)
  })
})

describe('openRealmError', () => {
  it('names the world, OK only — no Reload, and dismisses once', async () => {
    const onDismiss = vi.fn()
    render(<PopupHost />)
    act(() => {
      openRealmError({ message: 'The world "nope.dcl.eth" doesn\'t exist.', onDismiss })
    })
    expect(screen.getByText(/World not found/i)).toBeInTheDocument()
    expect(screen.getByText(/nope\.dcl\.eth/)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Reload/i })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: /^OK$/i }))
    expect(onDismiss).toHaveBeenCalledTimes(1)
    expect(screen.queryByText(/World not found/i)).toBeNull()
  })

  it('is an ordinary popup: a cancel-dismiss closes it and it never takes the input lock', () => {
    const onDismiss = vi.fn()
    render(<PopupHost />)
    act(() => {
      openRealmError({ message: 'The world "nope.dcl.eth" doesn\'t exist.', onDismiss })
    })
    // Unlike a crash, a world-not-found freezes nothing behind it.
    expect(isInputLocked()).toBe(false)
    // The cancel key resolves via the session (stream in-world, DOM fallback pre-world),
    // both landing on closeTopPopup — see systemActionShortcuts.test.tsx.
    act(() => closeTopPopup())
    expect(screen.queryByText(/World not found/i)).toBeNull()
    expect(onDismiss).toHaveBeenCalledTimes(1) // the dismiss contract still settles exactly once
  })
})

describe('ErrorBoundary', () => {
  function Boom(): React.JSX.Element {
    throw new Error('render exploded')
  }
  it('catches a render crash and shows the crash surface', () => {
    // Silence the expected React error log for this render.
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {})
    render(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>
    )
    expect(screen.getByText(/render exploded/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Reload/i })).toBeInTheDocument()
    spy.mockRestore()
  })
})
