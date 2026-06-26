import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { Community } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: communities — browse list, join/leave, per-community detail.
describe('communities domain', () => {
  const community: Community = {
    id: 'c1',
    name: 'Builders',
    description: 'we build',
    membersCount: 10,
    role: 'none',
    ownerName: 'Owner',
    privacy: 'public'
  }

  it('opening the panel fetches communities once', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().communities.toggle())
    expect(h.driver.sentOf('getCommunities')).toHaveLength(1)
  })

  it('communities stream populates the list', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'communities', communities: [community] })
    expect(h.session().communities.list[0].name).toBe('Builders')
  })

  it('join / leave post the community id', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().communities.join('c1'))
    expect(h.driver.last('joinCommunity')).toEqual({ kind: 'joinCommunity', id: 'c1' })
    act(() => h.session().communities.leave('c1'))
    expect(h.driver.last('leaveCommunity')).toEqual({ kind: 'leaveCommunity', id: 'c1' })
  })

  it('loadDetail requests detail and the stream fills the modal', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().communities.loadDetail('c1'))
    expect(h.driver.last('getCommunityDetail')).toEqual({ kind: 'getCommunityDetail', id: 'c1' })
    h.driver.emit({
      kind: 'communityDetail',
      id: 'c1',
      members: [{ address: '0x1', name: 'A', role: 'owner' }],
      posts: [],
      places: [],
      events: [],
      photos: []
    })
    expect(h.session().communities.detail?.id).toBe('c1')
    expect(h.session().communities.detail?.members).toHaveLength(1)
  })
})
