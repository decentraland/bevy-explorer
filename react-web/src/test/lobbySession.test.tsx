import { act, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { renderSession } from './harness'

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
})
