import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { Profile } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: profile — the local player's passport, fetched on world entry.
describe('profile domain', () => {
  const profile: Profile = {
    address: '0xme',
    name: 'Tester',
    hasClaimedName: true,
    isGuest: false,
    description: 'hi'
  }

  it('requests the profile on world entry', async () => {
    const h = renderSession()
    await enterAsGuest(h, { keepSent: true })
    expect(h.driver.sentOf('getProfile')).toHaveLength(1)
  })

  it('profile stream populates the passport', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'profile', profile })
    expect(h.session().profile.data).toMatchObject({ address: '0xme', name: 'Tester' })
  })

  it('requestUserProfile fetches a user and caches the response by address', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().requestUserProfile('0xKURD'))
    expect(h.driver.last('getUserProfile')).toEqual({ kind: 'getUserProfile', address: '0xKURD' })
    h.driver.emit({
      kind: 'userProfile',
      address: '0xKURD',
      profile: { address: '0xkurd', name: 'kurd', hasClaimedName: true, isGuest: false, description: 'gm' }
    })
    expect(h.session().userProfiles['0xkurd']?.name).toBe('kurd')
  })

  it('saves an edit optimistically, and reverts it when the engine rejects the deploy', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'profile', profile })
    act(() => h.session().requestUserProfile('0xme'))
    h.driver.emit({ kind: 'userProfile', address: '0xme', profile })

    act(() => h.session().profile.save({ description: 'gm from the beach' }))
    expect(h.driver.last('saveProfile')).toEqual({ kind: 'saveProfile', description: 'gm from the beach' })
    // Shown before the round trip — the deploy takes seconds, the catalyst reindex longer.
    expect(h.session().profile.saving).toBe(true)
    expect(h.session().profile.data?.description).toBe('gm from the beach')
    expect(h.session().userProfiles['0xme']?.description).toBe('gm from the beach')

    h.driver.emit({ kind: 'profileSaved', ok: false, error: 'failed to deploy to server.' })
    expect(h.session().profile.saving).toBe(false)
    expect(h.session().profile.saveError).toBe('failed to deploy to server.')
    expect(h.session().profile.data?.description).toBe('hi')
    expect(h.session().userProfiles['0xme']?.description).toBe('hi')

    act(() => h.session().profile.dismissSaveError())
    expect(h.session().profile.saveError).toBeNull()
  })

  it('keeps a successful edit and holds no error', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'profile', profile })
    h.driver.emit({ kind: 'ownedNames', names: ['Tester2'] })
    act(() => h.session().profile.save({ name: 'Tester2' }))
    h.driver.emit({ kind: 'profileSaved', ok: true })
    expect(h.session().profile.data).toMatchObject({ name: 'Tester2', hasClaimedName: true })
    expect(h.session().profile.saveError).toBeNull()
    expect(h.session().profile.saving).toBe(false)
  })

  it('relays the claimed names the picker offers', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().profile.requestOwnedNames())
    expect(h.driver.last('getOwnedNames')).toEqual({ kind: 'getOwnedNames' })
    h.driver.emit({ kind: 'ownedNames', names: ['Mojito'] })
    expect(h.session().profile.ownedNames).toEqual(['Mojito'])
  })

  it('toggles the profile panel open/closed', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().profile.open).toBe(false)
    act(() => h.session().profile.toggle())
    expect(h.session().profile.open).toBe(true)
  })
})
