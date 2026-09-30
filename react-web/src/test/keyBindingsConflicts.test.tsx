// The duplicate-binding warning skips overlaps that are shared by design: a quick emote only
// plays while the emote wheel is open, and the open wheel mutes scenes, so quick emotes may
// share keys with scene actions (the digit defaults overlap Action 3-6).
import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import { resetPopups } from '../design'
import { setBindingsSnapshot } from '../lib/bindingLabels'
import { KeyBindingsTab } from '../features/settings/KeyBindingsTab'
import type { BindingEntry } from '../engine/protocol'

afterEach(() => {
  resetPopups()
  act(() => setBindingsSnapshot([]))
})

function renderTab(table: BindingEntry[]): void {
  act(() => setBindingsSnapshot(table))
  render(<KeyBindingsTab bindings={{ list: table, set: vi.fn(), reset: vi.fn(), capture: vi.fn() }} />)
}

const titles = (label: string): string[] =>
  screen.getAllByRole('button', { name: label }).map((b) => b.getAttribute('title') ?? '')

describe('binding conflict warning', () => {
  it('a quick emote sharing a key with a scene action is not a conflict', () => {
    renderTab([
      [{ Scene: 'IaAction3' }, ['Digit1']],
      [{ System: 'QuickEmote1' }, ['Digit1']]
    ])
    expect(titles('1')).toEqual(['Click to rebind', 'Click to rebind'])
  })

  it('a quick emote sharing a key with another system action still is', () => {
    renderTab([
      [{ System: 'Places' }, ['Digit1']],
      [{ System: 'QuickEmote1' }, ['Digit1']]
    ])
    expect(titles('1')).toEqual([
      'Also bound to Quick Emote 1 — click to rebind',
      'Also bound to Places — click to rebind'
    ])
  })
})
