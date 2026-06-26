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

  it('toggles the profile panel open/closed', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().profile.open).toBe(false)
    act(() => h.session().profile.toggle())
    expect(h.session().profile.open).toBe(true)
  })
})
