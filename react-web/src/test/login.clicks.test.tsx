import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { GuestStartPage } from '../features/login/GuestStartPage'
import { LoadingAndLogin } from '../features/login/LoadingAndLogin'
import type { LoginFlow } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function flow(over: Partial<LoginFlow>): LoginFlow {
  return { ...fakeSession().login, ...over }
}

describe('login screen clicks', () => {
  it('no session: ACCEPT AND PLAY plays as a guest with the picked name and look, CONTINUE WITH AN ACCOUNT signs in', async () => {
    const f = flow({ status: 'sign-in-or-guest' })
    render(<GuestStartPage flow={f} />)
    const field = screen.getByRole('textbox', { name: 'Name' })
    const play = screen.getByRole('button', { name: /ACCEPT AND PLAY/i })
    await userEvent.click(play)
    const first = vi.mocked(f.exploreAsGuest).mock.calls[0][0]!
    expect(first.name).toBe((field as HTMLInputElement).value)
    // the dice picks another name and another look
    await userEvent.click(screen.getByRole('button', { name: 'Random name and look' }))
    await userEvent.click(play)
    const second = vi.mocked(f.exploreAsGuest).mock.calls[1][0]!
    expect(second.name).not.toBe(first.name)
    expect(second.look).not.toBe(first.look)

    await userEvent.clear(field)
    await userEvent.type(field, 'NovaRiverbend')
    await userEvent.click(play)
    expect(vi.mocked(f.exploreAsGuest)).toHaveBeenLastCalledWith({ name: 'NovaRiverbend', look: second.look })
    await userEvent.click(screen.getByRole('button', { name: /CONTINUE WITH AN ACCOUNT/i }))
    expect(vi.mocked(f.startWithAccount)).toHaveBeenCalledTimes(1)
  })

  it('a name that can not be claimed can not play', async () => {
    render(<GuestStartPage flow={flow({ status: 'sign-in-or-guest' })} />)
    const field = screen.getByRole('textbox', { name: 'Name' })
    await userEvent.clear(field)
    await userEvent.type(field, 'no spaces allowed')
    expect(screen.getByRole('button', { name: /ACCEPT AND PLAY/i })).toBeDisabled()
  })

  it('reuse-login-or-new: the splash covers the way to the lobby; after a failed sign-in, JUMP IN / USE A DIFFERENT ACCOUNT', async () => {
    const { unmount } = render(<LoadingAndLogin flow={flow({ status: 'reuse-login-or-new', account: '0xabc' })} />)
    expect(screen.queryByRole('button', { name: /JUMP INTO DECENTRALAND/i })).toBeNull()
    unmount()
    const f = flow({ status: 'reuse-login-or-new', account: '0xabc', error: 'Login failed' })
    render(<LoadingAndLogin flow={f} />)
    await userEvent.click(screen.getByRole('button', { name: /JUMP INTO DECENTRALAND/i }))
    expect(vi.mocked(f.jumpIn)).toHaveBeenCalledTimes(1)
    await userEvent.click(screen.getByRole('button', { name: /USE A DIFFERENT ACCOUNT/i }))
    expect(vi.mocked(f.useDifferentAccount)).toHaveBeenCalledTimes(1)
  })

  it('CTAs are disabled while busy', () => {
    render(<GuestStartPage flow={flow({ status: 'sign-in-or-guest', busy: true })} />)
    expect(screen.getByRole('button', { name: /ACCEPT AND PLAY/i })).toBeDisabled()
    expect(screen.getByRole('button', { name: /CONTINUE WITH AN ACCOUNT/i })).toBeDisabled()
  })

  it('shows the engine boot progress while loading, and holds ACCEPT AND PLAY until it is ready', () => {
    render(<LoadingAndLogin flow={flow({ status: 'loading', engineReady: false, loadProgress: 42, loadStep: 'download' })} />)
    const bar = screen.getByRole('progressbar')
    expect(bar).toHaveTextContent(/Downloading engine · 42%/i)
    expect(bar).toHaveAttribute('aria-valuenow', '42')

    const { rerender } = render(<GuestStartPage flow={flow({ status: 'sign-in-or-guest', engineReady: false, loadProgress: 42, loadStep: 'download' })} />)
    // The gated CTA shows the live download percent instead of "STARTING…".
    const gated = screen.getByRole('button', { name: /DOWNLOADING/i })
    expect(gated).toHaveTextContent(/DOWNLOADING…?\s*42%/i)
    expect(gated).toBeDisabled()
    rerender(<GuestStartPage flow={flow({ status: 'sign-in-or-guest', engineReady: true })} />)
    expect(screen.getByRole('button', { name: /ACCEPT AND PLAY/i })).toBeEnabled()
  })
})
