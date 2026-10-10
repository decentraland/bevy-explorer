import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { LaunchHostOptions } from '../engine/engineRpc'
import { SessionProvider } from '../features/session/SessionContext'
import { WelcomePage } from '../features/welcome/WelcomePage'
import { FakeDriver, fakeProfileState, fakeSession, renderSession, type Harness } from './harness'

// The web engine: launched by the page, which reads the terms state before it does.
class LaunchRecordingDriver extends FakeDriver {
  hosts: Array<LaunchHostOptions | undefined> = []
  constructor(private readonly terms: boolean | null) {
    super()
    this.welcome = null
  }
  launch(_realm?: string, _position?: string, host?: LaunchHostOptions): void {
    this.hosts.push(host)
  }
  termsAccepted(): boolean | null {
    return this.terms
  }
}

// A driver whose getWelcome the test answers itself.
function manual(): FakeDriver {
  const driver = new FakeDriver()
  driver.welcome = null
  return driver
}

async function signIn(h: Harness): Promise<void> {
  await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
  act(() => h.session().login.exploreAsGuest())
  await waitFor(() => expect(h.session().login.busy).toBe(false))
  await waitFor(() => expect(h.driver.sentOf('getWelcome')).toHaveLength(1))
}

async function withUrl(search: string, run: () => Promise<void>): Promise<void> {
  const url = new URL(location.href)
  history.replaceState(null, '', search)
  try {
    await run()
  } finally {
    history.replaceState(null, '', url.pathname + url.search)
  }
}

describe('welcome page', () => {
  it('shows for a new account after sign-in, then the lobby once accepted', async () => {
    const h = renderSession(undefined, manual())
    await signIn(h)
    // the lobby waits for the answer, behind the bare stage
    expect(h.session().phase).toBe('welcome')
    expect(h.session().welcome.pending).toBe(true)

    h.driver.emit({ kind: 'welcome', terms: true, newProfile: true })
    await waitFor(() => expect(h.session().phase).toBe('welcome'))
    // the page shows the name
    expect(h.driver.sentOf('getProfile')).toHaveLength(1)
    act(() => h.session().welcome.openLegal('terms'))
    expect(h.driver.last('openLegal')).toEqual({ kind: 'openLegal', doc: 'terms' })

    act(() => h.session().welcome.accept('NewName'))
    expect(h.driver.last('acceptWelcome')).toEqual({ kind: 'acceptWelcome', name: 'NewName' })
    expect(h.session().welcome.saving).toBe(true)

    h.driver.emit({ kind: 'welcomeAccepted', ok: true })
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    // the lobby keeps the world held until a destination is picked
    expect(h.driver.sentOf('teleport')).toHaveLength(0)
  })

  it('stays on the page with the error when the save fails', async () => {
    const h = renderSession(undefined, manual())
    await signIn(h)
    h.driver.emit({ kind: 'welcome', terms: false, newProfile: true })
    await waitFor(() => expect(h.session().phase).toBe('welcome'))

    act(() => h.session().welcome.accept('NewName'))
    h.driver.emit({ kind: 'welcomeAccepted', ok: false, error: 'failed to deploy to server.' })
    await waitFor(() => expect(h.session().welcome.error).toBe('failed to deploy to server.'))
    expect(h.session().welcome.saving).toBe(false)
    expect(h.session().phase).toBe('welcome')
  })

  it('shows the lobby once there is nothing to accept', async () => {
    const h = renderSession(undefined, manual())
    await signIn(h)
    expect(h.session().phase).toBe('welcome')
    h.driver.emit({ kind: 'welcome', terms: false, newProfile: false })
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
  })

  it('travels to a realm link held for the terms once they are accepted', async () => {
    await withUrl('/?realm=foo.dcl.eth&position=3,4', async () => {
      const h = renderSession({ userId: null }, manual())
      await signIn(h)
      h.driver.emit({ kind: 'welcome', terms: true, newProfile: false })
      await waitFor(() => expect(h.session().phase).toBe('welcome'))
      // an existing profile's name isn't on the page
      expect(h.driver.sentOf('getProfile')).toHaveLength(0)

      act(() => h.session().welcome.accept())
      expect(h.driver.last('acceptWelcome')).toEqual({ kind: 'acceptWelcome', name: undefined })
      h.driver.emit({ kind: 'welcomeAccepted', ok: true })
      await waitFor(() => expect(h.session().phase).toBe('entering'))
      expect(h.driver.last('teleport')).toMatchObject({ realm: 'foo.dcl.eth', x: 3, y: 4 })
    })
  })

  it('holds a web link for terms not yet accepted, and goes there once they are', async () => {
    await withUrl('/?position=10,20', async () => {
      const driver = new LaunchRecordingDriver(false)
      const h = renderSession({ userId: null }, driver)
      await signIn(h)
      expect(driver.hosts).toEqual([{ holdWorld: true }])
      h.driver.emit({ kind: 'welcome', terms: true, newProfile: false })
      await waitFor(() => expect(h.session().phase).toBe('welcome'))

      act(() => h.session().welcome.accept())
      h.driver.emit({ kind: 'welcomeAccepted', ok: true })
      await waitFor(() => expect(h.session().phase).toBe('entering'))
      expect(h.driver.last('teleport')).toMatchObject({ x: 10, y: 20 })
    })
  })

  it('releases a held web link straight away when there is nothing to show', async () => {
    await withUrl('/?position=10,20', async () => {
      const driver = new LaunchRecordingDriver(false)
      const h = renderSession({ userId: null }, driver)
      await signIn(h)
      h.driver.emit({ kind: 'welcome', terms: false, newProfile: false })
      await waitFor(() => expect(h.driver.last('teleport')).toMatchObject({ x: 10, y: 20 }))
    })
  })

  it("doesn't hold a web link once the terms are accepted", async () => {
    await withUrl('/?position=10,20', async () => {
      const driver = new LaunchRecordingDriver(true)
      const h = renderSession({ userId: null }, driver)
      await signIn(h)
      expect(driver.hosts).toEqual([undefined])
      h.driver.emit({ kind: 'welcome', terms: false, newProfile: false })
      await waitFor(() => expect(h.session().welcome.terms).toBe(false))
      expect(h.driver.sentOf('teleport')).toHaveLength(0)
    })
  })

  it('neither holds nor asks for the sites embed', async () => {
    await withUrl('/?hud=0&guest=1', async () => {
      const driver = new LaunchRecordingDriver(false)
      const h = renderSession({ userId: null }, driver)
      await waitFor(() => expect(driver.hosts).toHaveLength(1))
      await waitFor(() => expect(h.session().login.busy).toBe(false))
      expect(driver.hosts).toEqual([undefined])
      expect(h.driver.sentOf('getWelcome')).toHaveLength(0)
    })
  })

  it('forgets the page on sign-out', async () => {
    const h = renderSession(undefined, manual())
    await signIn(h)
    h.driver.emit({ kind: 'welcome', terms: true, newProfile: true })
    await waitFor(() => expect(h.session().phase).toBe('welcome'))
    act(() => h.session().logout())
    await waitFor(() => expect(h.session().phase).toBe('login'))
    expect(h.session().welcome.terms).toBe(false)
  })
})

describe('welcome page view', () => {
  const show = (welcome: { terms: boolean; newProfile: boolean; pending?: boolean }): ReturnType<typeof fakeSession> => {
    const session = fakeSession()
    session.welcome = { ...session.welcome, ...welcome }
    session.profile = fakeProfileState({
      data: { address: '0x1234', name: 'NovaStarfall#1234', hasClaimedName: false, isGuest: false }
    })
    render(
      <SessionProvider value={session}>
        <WelcomePage setEngineViewport={session.setEngineViewport} />
      </SessionProvider>
    )
    return session
  }

  it('shows only the background while the answer is pending', () => {
    show({ terms: false, newProfile: false, pending: true })
    expect(screen.queryByRole('region', { name: 'Welcome' })).toBeNull()
    expect(screen.queryByRole('button')).toBeNull()
  })

  it('offers only the terms to an existing profile', () => {
    const session = show({ terms: true, newProfile: false })
    expect(screen.queryByLabelText('Name')).toBeNull()
    expect(screen.queryByRole('button', { name: 'RANDOMIZE' })).toBeNull()
    expect(screen.queryByText(/customize your appearance/)).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Terms of Use' }))
    expect(session.welcome.openLegal).toHaveBeenCalledWith('terms')
    fireEvent.click(screen.getByRole('button', { name: 'ACCEPT AND PLAY' }))
    expect(session.welcome.accept).toHaveBeenCalledWith(undefined)
  })

  it('tells a new account its appearance can change once the terms are accepted', () => {
    show({ terms: true, newProfile: true })
    expect(screen.getByText('You can freely customize your appearance after accepting the terms.')).toBeInTheDocument()
  })

  it('names a new profile, without the suffix, and saves the edited name', () => {
    const session = show({ terms: false, newProfile: true })
    const field = screen.getByLabelText('Name')
    expect(field).toHaveValue('NovaStarfall')
    fireEvent.click(screen.getByRole('button', { name: 'RANDOMIZE' }))
    expect(session.welcome.reroll).toHaveBeenCalledOnce()
    // the terms are accepted already, so nothing waits on them
    expect(screen.queryByText(/customize your appearance/)).toBeNull()

    fireEvent.change(field, { target: { value: 'not valid!' } })
    expect(screen.getByRole('button', { name: 'PLAY' })).toBeDisabled()
    fireEvent.change(field, { target: { value: 'Renamed' } })
    fireEvent.click(screen.getByRole('button', { name: 'PLAY' }))
    expect(session.welcome.accept).toHaveBeenCalledWith('Renamed')
  })
})
