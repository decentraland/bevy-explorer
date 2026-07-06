import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { LoadingAndLogin } from '../features/login/LoadingAndLogin'
import type { LoginFlow } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function flow(over: Partial<LoginFlow>): LoginFlow {
  return { ...fakeSession().login, ...over }
}

describe('login screen clicks', () => {
  it('sign-in-or-guest: START WITH ACCOUNT / EXPLORE AS GUEST', async () => {
    const f = flow({ status: 'sign-in-or-guest' })
    render(<LoadingAndLogin flow={f} />)
    await userEvent.click(screen.getByRole('button', { name: /START WITH ACCOUNT/i }))
    expect(vi.mocked(f.startWithAccount)).toHaveBeenCalledTimes(1)
    await userEvent.click(screen.getByRole('button', { name: /EXPLORE AS GUEST/i }))
    expect(vi.mocked(f.exploreAsGuest)).toHaveBeenCalledTimes(1)
  })

  it('reuse-login-or-new: JUMP IN / USE A DIFFERENT ACCOUNT', async () => {
    const f = flow({ status: 'reuse-login-or-new', account: '0xabc' })
    render(<LoadingAndLogin flow={f} />)
    await userEvent.click(screen.getByRole('button', { name: /JUMP INTO DECENTRALAND/i }))
    expect(vi.mocked(f.jumpIn)).toHaveBeenCalledTimes(1)
    await userEvent.click(screen.getByRole('button', { name: /USE A DIFFERENT ACCOUNT/i }))
    expect(vi.mocked(f.useDifferentAccount)).toHaveBeenCalledTimes(1)
  })

  it('CTAs are disabled while busy', () => {
    render(<LoadingAndLogin flow={flow({ status: 'sign-in-or-guest', busy: true })} />)
    expect(screen.getByRole('button', { name: /EXPLORE AS GUEST/i })).toBeDisabled()
  })

  it('shows the engine boot progress bar while loading, hides it when ready', () => {
    const { rerender } = render(
      <LoadingAndLogin flow={flow({ status: 'sign-in-or-guest', engineReady: false, loadProgress: 42, loadStep: 'download' })} />
    )
    const bar = screen.getByRole('progressbar')
    expect(bar).toHaveTextContent(/Downloading engine · 42%/i)
    expect(bar).toHaveAttribute('aria-valuenow', '42')
    // The gated CTA shows the live download percent instead of "STARTING…".
    const gated = screen.getByRole('button', { name: /DOWNLOADING/i })
    expect(gated).toHaveTextContent(/DOWNLOADING…?\s*42%/i)

    rerender(<LoadingAndLogin flow={flow({ status: 'sign-in-or-guest', engineReady: true })} />)
    expect(screen.queryByRole('progressbar')).toBeNull()
    expect(screen.getByRole('button', { name: /EXPLORE AS GUEST/i })).toBeInTheDocument()
  })
})
