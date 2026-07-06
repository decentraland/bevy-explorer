import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { EngineErrorModal } from '../features/error/EngineErrorModal'
import { ErrorBoundary } from '../features/error/ErrorBoundary'

describe('EngineErrorModal', () => {
  it('shows the crash message + Reload, calls onReload', async () => {
    const onReload = vi.fn()
    render(<EngineErrorModal error={{ message: "can't init wasm queue", source: 'launch' }} onReload={onReload} />)
    expect(screen.getByText(/can't init wasm queue/i)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: /Reload/i }))
    expect(onReload).toHaveBeenCalledTimes(1)
  })

  it('shows Dismiss only when onDismiss is provided (runtime crash)', () => {
    const { rerender } = render(
      <EngineErrorModal error={{ message: 'boom', source: 'launch' }} onReload={vi.fn()} />
    )
    expect(screen.queryByRole('button', { name: /Dismiss/i })).toBeNull()
    rerender(<EngineErrorModal error={{ message: 'boom', source: 'runtime' }} onReload={vi.fn()} onDismiss={vi.fn()} />)
    expect(screen.getByRole('button', { name: /Dismiss/i })).toBeInTheDocument()
  })

  it('realm not-found: names the world, OK only — no Reload', async () => {
    const onDismiss = vi.fn()
    render(
      <EngineErrorModal
        error={{ message: 'The world "nope.dcl.eth" doesn\'t exist.', source: 'realm' }}
        onReload={vi.fn()}
        onDismiss={onDismiss}
      />
    )
    expect(screen.getByText(/World not found/i)).toBeInTheDocument()
    expect(screen.getByText(/nope\.dcl\.eth/)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Reload/i })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: /^OK$/i }))
    expect(onDismiss).toHaveBeenCalledTimes(1)
  })
})

describe('ErrorBoundary', () => {
  function Boom(): React.JSX.Element {
    throw new Error('render exploded')
  }
  it('catches a render crash and shows the error popup', () => {
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
