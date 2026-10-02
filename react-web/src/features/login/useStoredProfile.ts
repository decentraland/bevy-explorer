// The stored account's name + full-body snapshot, for the screens shown before the engine draws
// the live avatar. The by-address image redirects to a broken S3 path, so we resolve the real
// (hash-based) body URL from the profile (lambdas send CORS → fetch works under credentialless).

import { useEffect, useState } from 'react'
import { serviceUrl } from '../../lib/baseDomain'

interface ProfileResponse {
  avatars?: Array<{ name?: string; avatar?: { snapshots?: { body?: string } } }>
}
export function useStoredProfile(address?: string): { name?: string; body?: string } {
  const [data, setData] = useState<{ name?: string; body?: string }>({})
  useEffect(() => {
    if (address == null || address === '') {
      setData({})
      return
    }
    let cancelled = false
    fetch(`${serviceUrl('catalyst')}/lambdas/profiles/${address}`)
      .then((r) => r.json() as Promise<ProfileResponse>)
      .then((j) => {
        const a = j.avatars?.[0]
        if (!cancelled) setData({ name: a?.name, body: a?.avatar?.snapshots?.body })
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [address])
  return data
}
