import { describe, it, expect } from 'vitest'
import { act, render, screen, waitFor } from '@testing-library/react'
import { SceneLoadingOverlay } from '../features/session/SceneLoadingOverlay'
import { DEFAULT_REALM } from '../lib/baseDomain'
import { enterAsGuest, renderSession } from './harness'

// Audit P0 onboarding-realm-1: a failed in-world realm change stranded the player on "Reconnecting…".
// The engine now validates the destination before leaving the current realm and reports the outcome;
// the HUD shows its loader from the request until that outcome arrives.
describe('in-world realm change', () => {
  it('sends the change at once and shows the loader until the engine answers', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().phase).toBe('world')

    act(() => h.session().map.changeRealm('boedo.dcl.eth'))
    expect(h.driver.sentOf('changeRealm')).toEqual([{ kind: 'changeRealm', realm: 'boedo.dcl.eth', travelId: 1 }])
    expect(h.session().travellingTo).toBe('boedo.dcl.eth')
    expect(h.session().phase).toBe('entering')

    h.driver.emit({ kind: 'travelResult', travelId: 1, realm: 'boedo.dcl.eth', ok: true })
    expect(h.session().travellingTo).toBeNull()
    expect(h.session().travelError).toBeNull()
  })

  it('drops the loader and explains a failure; the player never left', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().map.changeRealm('noexiste.dcl.eth'))
    h.driver.emit({ kind: 'travelResult', travelId: 1, realm: 'noexiste.dcl.eth', ok: false, message: 'status: 404 Not Found' })
    expect(h.session().travellingTo).toBeNull()
    expect(h.session().travelError).toBe('Couldn\'t travel to "noexiste.dcl.eth": status: 404 Not Found')
    await waitFor(() => expect(h.session().phase).toBe('world'))
    act(() => h.session().dismissTravelError())
    expect(h.session().travelError).toBeNull()
  })

  it('only the latest travel counts: a superseded one neither drops the loader nor shows an error', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().map.changeRealm('first.dcl.eth'))
    act(() => h.session().map.changeRealm('second.dcl.eth'))
    h.driver.emit({ kind: 'travelResult', travelId: 1, realm: 'first.dcl.eth', ok: false, message: 'superseded by a later realm change' })
    expect(h.session().travellingTo).toBe('second.dcl.eth')
    expect(h.session().travelError).toBeNull()
  })

  it('/world and /goto genesis in chat travel the same way', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().chat.send('/world boedo.dcl.eth'))
    act(() => h.session().chat.send('/goto genesis'))
    expect(h.driver.sentOf('changeRealm')).toEqual([
      { kind: 'changeRealm', realm: 'boedo.dcl.eth', travelId: 1 },
      { kind: 'changeRealm', realm: DEFAULT_REALM, travelId: 2 }
    ])
    expect(h.session().travellingTo).toBe(DEFAULT_REALM)
  })

  it('the loader names the destination while travelling', () => {
    render(<SceneLoadingOverlay scene={{ visible: false, realmConnected: true, title: 'Genesis Plaza', pendingAssets: null }} progress={5} travellingTo="boedo.dcl.eth" />)
    expect(screen.getByText(/travelling to boedo\.dcl\.eth/i)).toBeInTheDocument()
    expect(screen.queryByText('Genesis Plaza')).toBeNull()
  })
})
