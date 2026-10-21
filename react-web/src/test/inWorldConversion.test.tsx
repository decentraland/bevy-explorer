// DOMAIN: the page→shell protocol behind the Google Ads conversion — the first scene of a page
// load posts `bevy-shell:in-world` with the login type, and later travels do not post again.

import { describe, expect, it, vi } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { postToShell } from '../lib/shell'
import { enterAsGuest, renderSession } from './harness'

vi.mock('../lib/shell', () => ({ inShell: true, postToShell: vi.fn(), shellRequest: vi.fn(async () => null) }))

const inWorldPosts = (): unknown[] => vi.mocked(postToShell).mock.calls.map(([m]) => m).filter((m) => m.type === 'bevy-shell:in-world')

describe('in-world conversion', () => {
  it('posts bevy-shell:in-world once per page load, with the login type', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h)
    expect(inWorldPosts()).toEqual([{ type: 'bevy-shell:in-world', login_type: 'guest' }])

    act(() => h.session().lobbyPage.travel({ kind: 'parcel', x: 5, y: 5 }))
    await waitFor(() => expect(h.session().phase).toBe('entering'))
    const go = h.driver.last('teleport')!
    h.driver.emit({ kind: 'travelResult', travelId: go.travelId!, realm: go.realm!, ok: true })
    await waitFor(() => expect(h.session().phase).toBe('world'))
    expect(inWorldPosts()).toHaveLength(1)
  })
})
