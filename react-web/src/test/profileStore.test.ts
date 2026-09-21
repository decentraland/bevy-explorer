import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  peekProfile,
  profileChanged,
  receiveProfile,
  requestPassport,
  resetProfileStore,
  seedProfiles,
  setProfileRequester,
  subscribeProfile
} from '../features/session/profileStore'
import type { Profile } from '../engine/protocol'

// STORE: the one address→identity map. Requests go out only for what's shown, once; a list's seed
// never overwrites the engine's copy; the engine's change stream re-reads only what's still shown.
const full = (over: Partial<Profile> = {}): Profile => ({
  address: '0xabc',
  name: 'Alice',
  picture: 'a.png',
  hasClaimedName: true,
  isGuest: false,
  version: 3,
  ...over
})

const request = vi.fn()
beforeEach(() => {
  resetProfileStore()
  request.mockClear()
  setProfileRequester(request)
})

describe('profile store', () => {
  it('asks the engine once for a shown address, however many subscribers arrive', () => {
    const off1 = subscribeProfile('0xabc', vi.fn())
    const off2 = subscribeProfile('0xabc', vi.fn())
    expect(request).toHaveBeenCalledTimes(1)
    expect(request).toHaveBeenCalledWith('0xabc', false)
    off1()
    off2()
  })

  it('does not ask for an address it already holds the engine copy of', () => {
    receiveProfile('0xABC', full())
    subscribeProfile('0xabc', vi.fn())
    expect(request).not.toHaveBeenCalled()
    expect(peekProfile('0xAbC')?.name).toBe('Alice') // matched case-insensitively
  })

  it('notifies subscribers when what it holds changes', () => {
    const cb = vi.fn()
    subscribeProfile('0xabc', cb)
    seedProfiles([{ address: '0xabc', name: 'Alice' }])
    expect(cb).toHaveBeenCalledTimes(1)
    seedProfiles([{ address: '0xabc', name: 'Alice' }]) // unchanged → no re-render
    expect(cb).toHaveBeenCalledTimes(1)
    receiveProfile('0xabc', full())
    expect(cb).toHaveBeenCalledTimes(2)
  })

  it('a list seed never overwrites the engine copy, and keeps a passport\'s extras through a re-read', () => {
    receiveProfile('0xabc', full({ badges: [{ id: 'b', name: 'Badge' }] }))
    seedProfiles([{ address: '0xabc', name: 'Stale' }])
    expect(peekProfile('0xabc')?.name).toBe('Alice')
    receiveProfile('0xabc', full({ name: 'Alicia', version: 4 })) // a plain identity re-read: no badges key
    expect(peekProfile('0xabc')?.name).toBe('Alicia')
    expect(peekProfile('0xabc')?.badges?.length).toBe(1)
  })

  it('the engine copy replaces a seed only where it says something', () => {
    seedProfiles([{ address: '0xabc', name: 'Alice', picture: 'a.png' }])
    receiveProfile('0xabc', full({ name: 'Alicia', picture: undefined }))
    expect(peekProfile('0xabc')).toMatchObject({ name: 'Alicia', picture: 'a.png', version: 3 })
    receiveProfile('0xabc', full({ name: '0xabc', picture: 'b.png', description: undefined }))
    expect(peekProfile('0xabc')).toMatchObject({ name: 'Alicia', picture: 'b.png' })
    expect(peekProfile('0xabc')?.description).toBeUndefined() // a cleared field is the engine's word
  })

  it('a "no profile" answer stands for a while rather than being re-asked per subscriber', () => {
    subscribeProfile('0xabc', vi.fn())
    receiveProfile('0xabc', null)
    subscribeProfile('0xabc', vi.fn())
    expect(request).toHaveBeenCalledTimes(1)
  })

  it('a passport request always goes out, with extras', () => {
    receiveProfile('0xabc', full())
    requestPassport('0xabc')
    expect(request).toHaveBeenCalledWith('0xabc', true)
  })

  it('profileChanged re-reads a shown profile only when what is held is older', () => {
    subscribeProfile('0xabc', vi.fn())
    receiveProfile('0xabc', full({ version: 3 }))
    request.mockClear()
    profileChanged('0xabc', 3)
    expect(request).not.toHaveBeenCalled()
    profileChanged('0xabc', 4)
    expect(request).toHaveBeenCalledWith('0xabc', false)
    expect(peekProfile('0xabc')?.version).toBe(3) // the old copy stays up until the reply lands
  })

  it('profileChanged drops a profile nobody is showing, and ignores one never seen', () => {
    receiveProfile('0xabc', full())
    profileChanged('0xabc', 9)
    expect(peekProfile('0xabc')).toBeUndefined()
    profileChanged('0xnew', 1)
    expect(request).not.toHaveBeenCalled()
  })

  it('does not ask the engine about anything but an address', () => {
    subscribeProfile('system', vi.fn())
    subscribeProfile('', vi.fn())
    expect(request).not.toHaveBeenCalled()
  })
})
