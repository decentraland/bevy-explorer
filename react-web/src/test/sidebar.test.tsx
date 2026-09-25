import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../features/sidebar/Sidebar'
import { fakeSession } from './harness'

// Unity parity for the nav rail (unity-explorer DCL/UI/Sidebar/SidebarView.cs).
describe('sidebar parity', () => {
  it.each(['Gallery', 'Places'])('%s uses the Unity art (mask), not a Material glyph', (name) => {
    render(<Sidebar session={fakeSession()} />)
    const button = screen.getByRole('button', { name })
    expect(button.querySelector('svg')).toBeNull()
    expect(button.querySelector('span[aria-hidden="true"]')).not.toBeNull()
  })

  it('Marketplace opens the shop with the client utm source, between Backpack and Gallery', async () => {
    const open = vi.spyOn(window, 'open').mockReturnValue(null)
    render(<Sidebar session={fakeSession()} />)
    const labels = screen.getAllByRole('button').map((b) => b.getAttribute('aria-label'))
    expect(labels.slice(labels.indexOf('Backpack'), labels.indexOf('Gallery') + 1)).toEqual(['Backpack', 'Marketplace', 'Gallery'])
    await userEvent.click(screen.getByRole('button', { name: 'Marketplace' }))
    expect(open).toHaveBeenCalledWith('https://decentraland.org/shop?utm_source=client', '_blank', 'noopener')
  })
})

afterEach(() => vi.restoreAllMocks())
