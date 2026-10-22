import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BootProgress } from '../features/engine/BootProgress'
import { GuestStartPage } from '../features/login/GuestStartPage'
import { LoadingAndLogin } from '../features/login/LoadingAndLogin'
import { SessionProvider } from '../features/session/SessionContext'
import type { EngineSession, LoginFlow } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function flow(over: Partial<LoginFlow>): LoginFlow {
  return { ...fakeSession().login, ...over }
}

// the guest start page reads the stage (and the look it shows) from the session
function guestStart(f: LoginFlow, session: EngineSession = fakeSession()): React.JSX.Element {
  return (
    <SessionProvider value={session}>
      <GuestStartPage flow={f} />
    </SessionProvider>
  )
}

// the curated default looks the guest start page fetches for its stage
const DEFAULT_LOOK = { bodyShape: 'urn:decentraland:off-chain:base-avatars:BaseFemale', wearables: ['urn:decentraland:off-chain:base-avatars:f_sweater'], eyes: { color: { r: 0.1, g: 0.2, b: 0.3 } } }

describe('login screen clicks', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', async (url: string) =>
      /\/lambdas\/profiles\/default\d+$/.test(url)
        ? new Response(JSON.stringify({ avatars: [{ name: url.split('/').pop(), avatar: { ...DEFAULT_LOOK, snapshots: { body: 'https://cdn/body.png' } } }] }), { status: 200 })
        : new Response(null, { status: 404 })
    )
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('no session: ACCEPT AND PLAY plays as a guest with the picked name and look, CONTINUE WITH AN ACCOUNT signs in', async () => {
    const f = flow({ status: 'sign-in-or-guest' })
    const session = fakeSession()
    render(guestStart(f, session))
    const field = screen.getByRole('textbox', { name: 'Name' })
    const play = screen.getByRole('button', { name: /ACCEPT AND PLAY/i })
    await userEvent.click(play)
    const first = vi.mocked(f.exploreAsGuest).mock.calls[0][0]!
    expect(first.name).toBe((field as HTMLInputElement).value)
    // the stage's avatar wears the look shown, as the page fetched it
    await waitFor(() => expect(session.welcome.showLook).toHaveBeenCalledTimes(1))
    expect(session.welcome.showLook).toHaveBeenCalledWith(first.look, {
      bodyShape: DEFAULT_LOOK.bodyShape,
      wearables: DEFAULT_LOOK.wearables,
      emotes: [],
      eyes: { r: 0.1, g: 0.2, b: 0.3 },
      hair: null,
      skin: null,
      forceRender: []
    })
    // the name's dice picks another name and nothing else; the look's dice another look, and holds
    // the stage from the click
    await userEvent.click(screen.getByRole('button', { name: 'Random name' }))
    expect(session.welcome.expectLook).not.toHaveBeenCalled()
    await userEvent.click(play)
    const renamed = vi.mocked(f.exploreAsGuest).mock.calls[1][0]!
    expect(renamed.name).not.toBe(first.name)
    expect(renamed.look).toBe(first.look)
    await userEvent.click(screen.getByRole('button', { name: 'Random look' }))
    expect(session.welcome.expectLook).toHaveBeenCalledTimes(1)
    await userEvent.click(play)
    const second = vi.mocked(f.exploreAsGuest).mock.calls[2][0]!
    expect(second.name).toBe(renamed.name)
    expect(second.look).not.toBe(first.look)
    await waitFor(() => expect(session.welcome.showLook).toHaveBeenCalledTimes(2))
    expect(vi.mocked(session.welcome.showLook).mock.calls[1][0]).toBe(second.look)
    // the dice showed the look fetched ahead, and fetches the one after it ahead too
    await waitFor(() => expect(session.welcome.preloadLook).toHaveBeenCalledTimes(2))
    const preloaded = vi.mocked(session.welcome.preloadLook).mock.calls.map((c) => c[0])
    expect(preloaded[0]).toBe(second.look)
    expect(preloaded[1]).not.toBe(second.look)

    await userEvent.clear(field)
    await userEvent.type(field, 'NovaRiverbend')
    await userEvent.click(play)
    expect(vi.mocked(f.exploreAsGuest)).toHaveBeenLastCalledWith({ name: 'NovaRiverbend', look: second.look })
    await userEvent.click(screen.getByRole('button', { name: /CONTINUE WITH AN ACCOUNT/i }))
    expect(vi.mocked(f.startWithAccount)).toHaveBeenCalledTimes(1)
  })

  it('a name that can not be claimed can not play', async () => {
    render(guestStart(flow({ status: 'sign-in-or-guest' })))
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
    render(guestStart(flow({ status: 'sign-in-or-guest', busy: true })))
    expect(screen.getByRole('button', { name: /ACCEPT AND PLAY/i })).toBeDisabled()
    expect(screen.getByRole('button', { name: /CONTINUE WITH AN ACCOUNT/i })).toBeDisabled()
  })

  it('shows the engine boot progress while loading; ACCEPT AND PLAY takes the click and shows the progress until the engine runs it', async () => {
    const { rerender: rerenderBar, unmount } = render(<BootProgress flow={flow({ status: 'loading', engineReady: false, avatarLoaded: false, loadProgress: 42, loadStep: 'download' })} />)
    const bar = screen.getByRole('progressbar')
    expect(bar).toHaveTextContent(/Downloading engine · 42%/i)
    expect(bar).toHaveAttribute('aria-valuenow', '42')
    // the engine can take commands but no avatar has loaded yet: held short of done
    rerenderBar(<BootProgress flow={flow({ status: 'sign-in-or-guest', engineReady: true, avatarLoaded: false, loadProgress: 100, loadStep: null })} />)
    expect(screen.getByRole('progressbar')).toHaveTextContent(/Starting… · 95%/i)
    // the first avatar loaded: full for a moment, then gone
    rerenderBar(<BootProgress flow={flow({ status: 'sign-in-or-guest', engineReady: true, avatarLoaded: true, loadProgress: 100, loadStep: null })} />)
    expect(screen.getByRole('progressbar')).toHaveTextContent(/Ready · 100%/i)
    await waitFor(() => expect(screen.queryByRole('progressbar')).toBeNull(), { timeout: 2000 })
    unmount()

    const f = flow({ status: 'sign-in-or-guest', engineReady: false, loadProgress: 42, loadStep: 'download' })
    const { rerender } = render(guestStart(f))
    const play = screen.getByRole('button', { name: /ACCEPT AND PLAY/i })
    expect(play).toBeEnabled()
    await userEvent.click(play)
    expect(vi.mocked(f.exploreAsGuest)).toHaveBeenCalledTimes(1)
    // the click is kept: the CTA dims and shows the live download percent instead of "STARTING…"
    rerender(guestStart({ ...f, acceptQueued: true }))
    const gated = screen.getByRole('button', { name: /DOWNLOADING/i })
    expect(gated).toHaveTextContent(/DOWNLOADING…?\s*42%/i)
    expect(gated).toBeDisabled()
    expect(screen.getByRole('textbox', { name: 'Name' })).toBeDisabled()
    expect(screen.getByRole('button', { name: /CONTINUE WITH AN ACCOUNT/i })).toBeDisabled()
    rerender(guestStart(flow({ status: 'sign-in-or-guest', engineReady: true })))
    expect(screen.getByRole('button', { name: /ACCEPT AND PLAY/i })).toBeEnabled()
  })

  it("the look's dice waits for the look it asked for to land on the live stage; the name's never waits", () => {
    const session = fakeSession()
    session.welcome = { ...session.welcome, lookPending: true }
    const { rerender } = render(guestStart(flow({ status: 'sign-in-or-guest' }), session))
    // the snapshot stage swaps instantly, so nothing waits before the live stage is up
    expect(screen.getByRole('button', { name: 'Random look' })).toBeEnabled()
    rerender(guestStart(flow({ status: 'sign-in-or-guest' }), { ...session, lobbyStageReady: true }))
    expect(screen.getByRole('button', { name: 'Random look' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Random name' })).toBeEnabled()
    rerender(guestStart(flow({ status: 'sign-in-or-guest' }), { ...session, lobbyStageReady: true, welcome: { ...session.welcome, lookPending: false } }))
    expect(screen.getByRole('button', { name: 'Random look' })).toBeEnabled()
  })
})
