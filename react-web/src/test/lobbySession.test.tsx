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
})
