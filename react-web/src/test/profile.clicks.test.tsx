import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfilePanel } from '../features/profile/ProfilePanel'
import type { ProfileState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function panel(over: Partial<ProfileState>): ProfileState {
  return { ...fakeSession().profile, open: true, toggle: vi.fn(), ...over }
}

describe('profile panel clicks', () => {
  it('close button toggles the panel', async () => {
    const p = panel({ data: { address: '0xme', name: 'Me', hasClaimedName: false, isGuest: true } })
    render(<ProfilePanel profile={p} />)
    await userEvent.click(screen.getByRole('button', { name: /Close profile/i }))
    expect(vi.mocked(p.toggle)).toHaveBeenCalledTimes(1)
  })

  it('renders profile links as external anchors', () => {
    const p = panel({
      data: {
        address: '0xme',
        name: 'Me',
        hasClaimedName: true,
        isGuest: false,
        links: [{ title: 'Twitter', url: 'https://x.com/me' }]
      }
    })
    render(<ProfilePanel profile={p} />)
    const link = screen.getByRole('link', { name: 'Twitter' })
    expect(link).toHaveAttribute('href', 'https://x.com/me')
    expect(link).toHaveAttribute('target', '_blank')
  })

  it('shows the unavailable state with no profile data', () => {
    render(<ProfilePanel profile={panel({ data: null })} />)
    expect(screen.getByText(/Profile unavailable/i)).toBeInTheDocument()
  })
})
