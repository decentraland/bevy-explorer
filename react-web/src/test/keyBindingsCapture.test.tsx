// The capture modal has no cancel-key handling of its own: the key flows to the engine and
// comes back as the CAPTURED input, and rebind interprets a Cancel-bound result as "abort"
// (captureCancelInputs). This is also what lets a gamepad-bound Cancel abort a capture.
import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import { PopupHost, resetPopups } from '../design'
import { setBindingsSnapshot } from '../lib/bindingLabels'
import { KeyBindingsTab } from '../features/settings/KeyBindingsTab'
import type { BindingEntry } from '../engine/protocol'
import type { BindingsState } from '../features/session/useEngineSession'

afterEach(() => {
  resetPopups()
  act(() => setBindingsSnapshot([]))
})

function renderTab(table: BindingEntry[]): { set: ReturnType<typeof vi.fn>; resolve: (v: string) => Promise<void> } {
  act(() => setBindingsSnapshot(table))
  let resolveInput!: (v: string) => void
  const set = vi.fn()
  const bindings: BindingsState = {
    list: table,
    set,
    reset: vi.fn(),
    capture: vi.fn(() => ({
      input: new Promise<string>((r) => (resolveInput = r)),
      cancel: vi.fn()
    }))
  }
  render(
    <>
      <PopupHost />
      <KeyBindingsTab bindings={bindings} />
    </>
  )
  return { set, resolve: (v) => act(async () => resolveInput(v)) }
}

const TABLE: BindingEntry[] = [
  [{ System: 'Cancel' }, ['Escape']],
  [{ System: 'Places' }, ['KeyZ']]
]

describe('capture cancel-by-result', () => {
  it('a Cancel-bound result aborts the capture: modal closes, nothing is bound', async () => {
    const { set, resolve } = renderTab(TABLE)
    act(() => screen.getByRole('button', { name: 'Z' }).click())
    expect(screen.getByText('Rebind Places')).toBeTruthy()
    expect(screen.getByText('Esc to cancel')).toBeTruthy()
    await resolve('Escape')
    expect(screen.queryByText('Rebind Places')).toBeNull()
    expect(set).not.toHaveBeenCalled()
  })

  it('a gamepad-bound Cancel aborts too (and is the shown hint)', async () => {
    const { set, resolve } = renderTab([
      [{ System: 'Cancel' }, ['Gamepad Select']],
      [{ System: 'Places' }, ['KeyZ']]
    ])
    act(() => screen.getByRole('button', { name: 'Z' }).click())
    expect(screen.getByText('Pad Select to cancel')).toBeTruthy()
    await resolve('Gamepad Select')
    expect(screen.queryByText('Rebind Places')).toBeNull()
    expect(set).not.toHaveBeenCalled()
  })

  it('any other result binds normally', async () => {
    const { set, resolve } = renderTab(TABLE)
    act(() => screen.getByRole('button', { name: 'Z' }).click())
    await resolve('KeyX')
    expect(set).toHaveBeenCalledWith([
      [{ System: 'Cancel' }, ['Escape']],
      [{ System: 'Places' }, ['KeyX']]
    ])
  })

  it('capturing FOR Cancel: its own key is a capturable input, not an abort', async () => {
    const { set, resolve } = renderTab(TABLE)
    act(() => screen.getByRole('button', { name: 'Esc' }).click())
    expect(screen.getByText('Rebind Cancel')).toBeTruthy()
    expect(screen.queryByText(/to cancel$/)).toBeNull() // nothing aborts this capture
    await resolve('Escape')
    expect(set).toHaveBeenCalledWith(TABLE) // Escape re-bound onto Cancel
  })
})
