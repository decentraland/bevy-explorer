import { describe, it, expect, vi, afterEach } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../features/sidebar/Sidebar'
import { fakeSession } from './harness'

function renderSidebar(over: { enabled?: boolean; available?: boolean } = {}) {
  const s = fakeSession()
  s.mic = { ...s.mic, available: true, enabled: false, toggle: vi.fn(), ...over }
  s.settings = { ...s.settings, list: [{ name: 'Voice', category: 'audio', description: '', minValue: 0, maxValue: 100, namedVariants: [], value: 70, default: 100, stepSize: 1 }], set: vi.fn(), load: vi.fn() }
  render(<Sidebar session={s} />)
  return s
}

afterEach(() => localStorage.clear())

describe('nearby voice', () => {
  it('the voice button opens the popover instead of toggling the mic', async () => {
    const s = renderSidebar()
    await userEvent.click(screen.getByRole('button', { name: 'Nearby Voice' }))
    expect(screen.getByRole('dialog', { name: 'Nearby voice' })).toBeInTheDocument()
    expect(vi.mocked(s.mic.toggle)).not.toHaveBeenCalled()
    expect(vi.mocked(s.settings.load)).toHaveBeenCalled()
  })

  it('Speak opens the mic, and the hint follows it', async () => {
    const s = renderSidebar()
    await userEvent.click(screen.getByRole('button', { name: 'Nearby Voice' }))
    await userEvent.click(screen.getByRole('button', { name: 'Speak' }))
    expect(vi.mocked(s.mic.toggle)).toHaveBeenCalledTimes(1)
  })

  it('the slider sets the incoming voice volume', async () => {
    const s = renderSidebar()
    await userEvent.click(screen.getByRole('button', { name: 'Nearby Voice' }))
    const slider = screen.getByRole('slider', { name: 'Voice volume' })
    expect(slider).toHaveValue('70')
    fireEvent.change(slider, { target: { value: '40' } })
    expect(vi.mocked(s.settings.set)).toHaveBeenLastCalledWith('Voice', 40)
  })

  it('turning hearing off mutes voices, closes the popover, and turning it on restores the volume', async () => {
    const s = renderSidebar({ enabled: true })
    await userEvent.click(screen.getByRole('button', { name: 'Nearby Voice' }))
    await userEvent.click(screen.getByRole('switch', { name: 'Hear others' }))
    expect(vi.mocked(s.settings.set)).toHaveBeenLastCalledWith('Voice', 0)
    expect(vi.mocked(s.mic.toggle)).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('dialog', { name: 'Nearby voice' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Nearby Voice' })).toHaveAttribute('data-voice', 'off')
    await userEvent.click(screen.getByRole('button', { name: 'Nearby Voice' }))
    await userEvent.click(screen.getByRole('switch', { name: 'Hear others' }))
    expect(vi.mocked(s.settings.set)).toHaveBeenLastCalledWith('Voice', 70)
  })

  it('without a microphone, Speak is disabled and says why', async () => {
    renderSidebar({ available: false })
    await userEvent.click(screen.getByRole('button', { name: 'Nearby Voice' }))
    expect(screen.getByRole('button', { name: 'Speak' })).toBeDisabled()
    expect(screen.getByText('No microphone available')).toBeInTheDocument()
  })
})
