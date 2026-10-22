// DOMAIN: the session's analytics call sites — the sign-in flow reports the screens and clicks
// mobile's lobby does, in order, with the same property shapes (queued; no engine is attached).

import { afterEach, describe, expect, it } from 'vitest'
import { act, waitFor } from '@testing-library/react'
import { pending } from '../lib/analytics'
import { renderSession } from './harness'

const entryUrl = location.pathname + location.search
afterEach(() => history.replaceState(null, '', entryUrl))

const names = (from: number): string[] =>
  pending()
    .slice(from)
    .map((e) => `${e.event}:${String(e.properties.button_text ?? e.properties.screen_name)}`)

describe('session analytics', () => {
  it('play as guest: AVATAR_NAMING → Click LETS_GO → AUTH_SUCCESS guest → LOBBY', async () => {
    const from = pending().length
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest('QuillStonehill'))
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    await waitFor(() => expect(h.session().login.busy).toBe(false))

    expect(names(from)).toEqual(['Screen Viewed:AVATAR_NAMING', 'Click Button:LETS_GO', 'Screen Viewed:AUTH_SUCCESS', 'Screen Viewed:LOBBY'])
    expect(pending()[from + 2].properties).toEqual({ screen_name: 'AUTH_SUCCESS', extra_properties: '{"login_type":"guest"}' })
  })

  it('a friend request reports the receiver', async () => {
    const h = renderSession({ userId: null })
    await waitFor(() => expect(h.session().login.status).toBe('sign-in-or-guest'))
    act(() => h.session().login.exploreAsGuest())
    await waitFor(() => expect(h.session().phase).toBe('lobby'))
    act(() => h.session().friends.act('request', '0xFRIEND'))

    const last = pending()[pending().length - 1]
    expect(last.event).toBe('Friend Request')
    expect(last.properties).toEqual({ receiver_id: '0xfriend' })
  })
})
