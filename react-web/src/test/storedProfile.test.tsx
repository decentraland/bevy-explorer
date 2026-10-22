import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useStoredProfile } from '../features/login/useStoredProfile'

const ADDRESS = '0x1234567890123456789012345678901234567890'
const missing = (): Response => new Response('{"error":"Not Found","message":"Profile not found"}', { status: 404 })
const found = (): Response => new Response(JSON.stringify({ avatars: [{ name: 'Quill', avatar: { bodyShape: 'urn:body', wearables: [] } }] }))

// The catalyst's answers for the address, in order (the last repeats); other fetches pass through.
function answering(answers: Array<() => Response>): { asked: () => number; restore: () => void } {
  const real = globalThis.fetch
  let n = 0
  globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    if (!String(input instanceof Request ? input.url : input).includes('/lambdas/profiles/')) return real(input, init)
    const answer = answers[Math.min(n++, answers.length - 1)]
    return Promise.resolve(answer())
  }) as typeof fetch
  return { asked: () => n, restore: () => (globalThis.fetch = real) }
}

describe('useStoredProfile', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it("settles on the catalyst's answer: the profile, or none at the address", async () => {
    const stub = answering([missing])
    try {
      const { result } = renderHook(() => useStoredProfile(ADDRESS))
      await waitFor(() => expect(result.current.settled).toBe(true))
      expect(result.current.exists).toBe(false)
      expect(stub.asked()).toBe(1)
    } finally {
      stub.restore()
    }
  })

  // Back from the auth site, the profile it made can still be syncing: a missing one is asked for
  // again, for the wait, and the stored sign-in (useEngineSession) holds until then.
  it('keeps asking for a missing profile within the wait, and settles once it is there', async () => {
    vi.useFakeTimers()
    const stub = answering([missing, missing, found])
    try {
      const { result } = renderHook(() => useStoredProfile(ADDRESS, 30_000))
      await act(() => vi.advanceTimersByTimeAsync(0))
      expect(result.current.settled).toBeUndefined()
      await act(() => vi.advanceTimersByTimeAsync(2_000))
      expect(result.current.settled).toBeUndefined()
      await act(() => vi.advanceTimersByTimeAsync(2_000))
      expect(result.current.settled).toBe(true)
      expect(result.current.exists).toBe(true)
      expect(result.current.name).toBe('Quill')
      expect(stub.asked()).toBe(3)
    } finally {
      stub.restore()
    }
  })

  it('gives up on a profile still missing after the wait', async () => {
    vi.useFakeTimers()
    const stub = answering([missing])
    try {
      const { result } = renderHook(() => useStoredProfile(ADDRESS, 3_000))
      await act(() => vi.advanceTimersByTimeAsync(0))
      await act(() => vi.advanceTimersByTimeAsync(2_000))
      expect(result.current.settled).toBeUndefined()
      await act(() => vi.advanceTimersByTimeAsync(2_000))
      expect(result.current.settled).toBe(true)
      expect(result.current.exists).toBe(false)
      expect(stub.asked()).toBe(3)
    } finally {
      stub.restore()
    }
  })
})
