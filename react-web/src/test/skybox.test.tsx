import { describe, it, expect, vi } from 'vitest'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../features/sidebar/Sidebar'
import { SkyboxMenu, formatHours } from '../features/skybox/SkyboxMenu'
import { enterAsGuest, fakeSession, renderSession } from './harness'

describe('skybox', () => {
  it('sidebar has Skybox between Nearby Voice and Emotes, and it toggles the menu', async () => {
    const s = fakeSession()
    render(<Sidebar session={s} />)
    const labels = screen.getAllByRole('button').map((b) => b.getAttribute('aria-label'))
    expect(labels.slice(labels.indexOf('Nearby Voice'), labels.indexOf('Emotes') + 1)).toEqual(['Nearby Voice', 'Skybox', 'Emotes'])
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

  it('closes on a click outside it, but not on itself or the rail', async () => {
    const skybox = { ...fakeSession().skybox, open: true }
    render(
      <>
        <nav aria-label="Main navigation"><button type="button">Skybox</button></nav>
        <SkyboxMenu skybox={skybox} />
        <div data-testid="world" />
      </>
    )
    await userEvent.click(screen.getByRole('switch'))
    await userEvent.click(screen.getByRole('button', { name: 'Skybox' }))
    expect(skybox.toggle).not.toHaveBeenCalled()
    await userEvent.click(screen.getByTestId('world'))
    expect(skybox.toggle).toHaveBeenCalledTimes(1)
  })

  it('follows the running clock while open, wrapping past midnight', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.commandReply = () => 'time 23:45 -> 23:45, speed 12 (elapsed: 85500)'
    act(() => h.session().skybox.toggle())
    await waitFor(() => expect(h.session().skybox.hours).toBe(23.75))

    h.driver.commandReply = () => 'time 0:15 -> 0:15, speed 12 (elapsed: 900)'
    await waitFor(() => expect(h.session().skybox.hours).toBe(0.25), { timeout: 2000 })

    // Frozen: the menu stops reading the clock.
    act(() => h.session().skybox.setProgressing(false))
    await waitFor(() => expect(h.driver.commands).toContain('/time 0.25 0'))
    const reads = h.driver.commands.length
    await new Promise((r) => setTimeout(r, 1200))
    expect(h.driver.commands).toHaveLength(reads)
  })

  it('formats hours as HH:MM', () => {
    expect(formatHours(0)).toBe('00:00')
    expect(formatHours(9.75)).toBe('09:45')
    expect(formatHours(24)).toBe('00:00')
  })
})
