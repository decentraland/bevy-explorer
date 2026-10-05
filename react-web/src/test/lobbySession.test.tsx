import { act, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { DEFAULT_REALM } from '../lib/baseDomain'
import { enterAsGuest, renderSession } from './harness'

describe('lobby session regressions', () => {
  it('waits for playerReady before opening and fetching Backpack', async () => {
    const h = renderSession()
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    await waitFor(() => expect(h.session().login.busy).toBe(false))

    act(() => h.session().backpack.toggle())
    expect(h.session().backpack.open).toBe(false)
    expect(h.driver.sentOf('getWearables')).toHaveLength(0)
    expect(h.driver.sentOf('getEmotes')).toHaveLength(0)
    expect(h.driver.sentOf('getOutfits')).toHaveLength(0)

    h.driver.emit({ kind: 'event', name: 'playerReady' })
    act(() => h.session().backpack.toggle())
    expect(h.session().backpack.open).toBe(true)
    expect(h.driver.sentOf('getWearables')).toHaveLength(1)
    expect(h.driver.sentOf('getEmotes')).toHaveLength(1)
    expect(h.driver.sentOf('getOutfits')).toHaveLength(1)
  })

  it('opens the native startup lobby after a fresh sign-in, not the places picker', async () => {
    const h = renderSession()
    Object.assign(h.driver, { loginNew: async () => {} })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    await act(async () => h.session().login.startWithAccount())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
  })

  it('returns to the native startup lobby after failed world travel and permits retry', async () => {
    const h = renderSession()
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    await waitFor(() => expect(h.session().login.busy).toBe(false))
    h.driver.emit({ kind: 'event', name: 'playerReady' })
    h.driver.emit({ kind: 'sceneLoading', state: { visible: false, realmConnected: true, title: '', pendingAssets: null } })

    act(() => h.session().pickDestination({ kind: 'world', realm: 'unreachable.dcl.eth' }))
    expect(h.session().phase).toBe('entering')
    const first = h.driver.last('changeRealm')!
    h.driver.emit({ kind: 'travelResult', travelId: first.travelId!, realm: first.realm, ok: false, message: 'unreachable' })
    await waitFor(() => expect(h.session().phase).toBe('lobby'))

    act(() => h.session().pickDestination({ kind: 'world', realm: 'home.dcl.eth' }))
    const second = h.driver.last('changeRealm')!
    expect(second.travelId).not.toBe(first.travelId)
    h.driver.emit({ kind: 'travelResult', travelId: second.travelId!, realm: second.realm, ok: true })
    await waitFor(() => expect(h.session().phase).toBe('world'))
  })

  it.each(['my-home.dcl.eth', DEFAULT_REALM])('waits for the native home response and returns to its saved parcel in %s', async (realm) => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().lobbyPage.toggle())
    act(() => h.session().lobbyPage.travel(null))
    expect(h.driver.sentOf('teleport')).toHaveLength(0)
    expect(h.session().lobbyPage.open).toBe(true)
    expect(h.driver.sent).toContainEqual({ kind: 'getHomeScene' })

    h.driver.emit({ kind: 'homeScene', realm, parcel: { x: 12, y: -7 } })
    expect(h.session().homeScene()).toEqual({ realm: realm === DEFAULT_REALM ? null : realm, parcel: '12,-7' })
    act(() => h.session().lobbyPage.travel(null))
    expect(h.driver.last('teleport')).toMatchObject({ realm, x: 12, y: -7 })
    expect(h.session().lobbyPage.open).toBe(false)

    act(() => h.session().lobbyPage.toggle())
    expect(h.session().homeScene()).toBeNull()
    h.driver.emit({ kind: 'homeScene', realm: 'new-home.dcl.eth', parcel: { x: 3, y: 4 } })
    act(() => h.session().lobbyPage.travel(null))
    expect(h.driver.last('teleport')).toMatchObject({ realm: 'new-home.dcl.eth', x: 3, y: 4 })
  })
})
