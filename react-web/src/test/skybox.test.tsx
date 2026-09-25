import { describe, it, expect, vi } from 'vitest'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../features/sidebar/Sidebar'
import { SkyboxMenu, formatHours } from '../features/skybox/SkyboxMenu'
import { enterAsGuest, fakeSession, renderSession } from './harness'

describe('skybox', () => {
  it('sidebar has Skybox between Voice chat and Emotes, and it toggles the menu', async () => {
    const s = fakeSession()
    render(<Sidebar session={s} />)
    const labels = screen.getAllByRole('button').map((b) => b.getAttribute('aria-label'))
    expect(labels.slice(labels.indexOf('Voice chat'), labels.indexOf('Emotes') + 1)).toEqual(['Voice chat', 'Skybox', 'Emotes'])
    await userEvent.click(screen.getByRole('button', { name: 'Skybox' }))
    expect(vi.mocked(s.skybox.toggle)).toHaveBeenCalledTimes(1)
  })

  it('locks the slider while time progression is on', async () => {
    const skybox = { ...fakeSession().skybox, open: true, hours: 18.5 }
    render(<SkyboxMenu skybox={skybox} />)
    expect(screen.getByText('18:30')).toBeInTheDocument()
    expect(screen.getByRole('slider')).toBeDisabled()
    await userEvent.click(screen.getByRole('switch'))
    expect(skybox.setProgressing).toHaveBeenCalledWith(false)
  })

  it('reads the live clock on open, freezes it where it is, and drives /time off the chat path', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.commandReply = () => 'time 18:30 -> 18:30, speed 12 (elapsed: 66600)'
    act(() => h.session().skybox.toggle())
    await waitFor(() => expect(h.session().skybox.hours).toBe(18.5))
    expect(h.session().skybox.progressing).toBe(true)

    // The clock ran on since the menu opened: freezing re-reads it rather than using the slider.
    h.driver.commandReply = () => 'time 19:0 -> 19:0, speed 12 (elapsed: 68400)'
    act(() => h.session().skybox.setProgressing(false))
    await waitFor(() => expect(h.driver.commands).toContain('/time 19.00 0'))
    expect(h.session().skybox.hours).toBe(19)

    act(() => h.session().skybox.setHours(20))
    act(() => h.session().skybox.setProgressing(true))
    expect(h.driver.commands).toEqual(['/time', '/time', '/time 19.00 0', '/time 20.00 0', '/time 20.00 12'])
    expect(h.driver.sentOf('consoleCommand')).toHaveLength(0)
  })

  it('formats hours as HH:MM', () => {
    expect(formatHours(0)).toBe('00:00')
    expect(formatHours(9.75)).toBe('09:45')
    expect(formatHours(24)).toBe('00:00')
  })
})
