import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { Sidebar } from '../features/sidebar/Sidebar'
import { fakeSession } from './harness'

const nav = (): HTMLElement => screen.getByRole('navigation', { name: 'Main navigation' })

describe('sidebar auto-hide (Unity SidebarConfigPanel)', () => {
  beforeEach(() => localStorage.clear())
  afterEach(() => vi.useRealTimers())

  it('the ••• button leads the rail and opens the auto-hide setting', () => {
    render(<Sidebar session={fakeSession()} />)
    expect(screen.getAllByRole('button')[0]).toHaveAccessibleName('Sidebar settings')
    fireEvent.click(screen.getByRole('button', { name: 'Sidebar settings' }))
    expect(screen.getByRole('switch', { name: 'Auto-hide sidebar' })).not.toBeChecked()
  })

  it('hides 0.3s after the pointer leaves and returns 0.3s after reaching the left edge', () => {
    vi.useFakeTimers()
    render(<Sidebar session={fakeSession()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Sidebar settings' }))
    fireEvent.click(screen.getByRole('switch', { name: 'Auto-hide sidebar' }))
    fireEvent.click(screen.getByRole('button', { name: 'Sidebar settings' }))

    fireEvent.pointerLeave(nav())
    act(() => vi.advanceTimersByTime(250))
    expect(nav()).toHaveAttribute('data-hidden', 'false')
    act(() => vi.advanceTimersByTime(100))
    expect(nav()).toHaveAttribute('data-hidden', 'true')

    fireEvent.pointerEnter(screen.getByTestId('sidebar-reveal'))
    act(() => vi.advanceTimersByTime(300))
    expect(nav()).toHaveAttribute('data-hidden', 'false')
  })

  it('remembers the choice across remounts (the rail unmounts under full-screen pages)', () => {
    const { unmount } = render(<Sidebar session={fakeSession()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Sidebar settings' }))
    fireEvent.click(screen.getByRole('switch', { name: 'Auto-hide sidebar' }))
    unmount()
    render(<Sidebar session={fakeSession()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Sidebar settings' }))
    expect(screen.getByRole('switch', { name: 'Auto-hide sidebar' })).toBeChecked()
  })
})
