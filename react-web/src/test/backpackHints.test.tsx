import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BackpackPage } from '../features/backpack/BackpackPage'
import { fakeProfileState, fakeSession } from './harness'

function renderBackpack(initialTab?: 'emotes') {
  const s = fakeSession()
  const emotes = { ...s.emotes, list: [{ urn: 'urn:wave', name: 'Wave' }], equip: vi.fn() }
  render(<BackpackPage backpack={{ ...s.backpack, open: true }} emotes={emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} initialTab={initialTab} />)
  return emotes
}

describe('backpack hints', () => {
  it('the controls button toggles the hints and an outside click closes them', async () => {
    renderBackpack()
    const button = screen.getByRole('button', { name: 'Controls' })
    await userEvent.click(button)
    expect(screen.getByText('Left-click and drag avatar to rotate.')).toBeInTheDocument()
    await userEvent.click(document.body)
    expect(screen.queryByText('Left-click and drag avatar to rotate.')).toBeNull()
  })

  it('pressing a number key over an emote puts it in that wheel slot', () => {
    const emotes = renderBackpack('emotes')
    fireEvent.mouseEnter(screen.getByRole('button', { name: 'Wave' }))
    fireEvent.keyDown(window, { key: '3' })
    expect(emotes.equip).toHaveBeenCalledWith(3, 'urn:wave')
  })

  it('number keys do nothing when no emote is hovered', () => {
    const emotes = renderBackpack('emotes')
    const card = screen.getByRole('button', { name: 'Wave' })
    fireEvent.mouseEnter(card)
    fireEvent.mouseLeave(card)
    fireEvent.keyDown(window, { key: '3' })
    expect(emotes.equip).not.toHaveBeenCalled()
  })
})
