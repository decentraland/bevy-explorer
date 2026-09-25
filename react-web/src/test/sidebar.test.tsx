import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
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
})
