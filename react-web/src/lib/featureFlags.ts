// Remote explorer feature flags (feature-flags.<base domain>/explorer.json), fetched once per
// session. Keys come back prefixed with `explorer-`; callers pass the bare name.

import { useEffect, useState } from 'react'
import { BASE_DOMAIN } from './baseDomain'

export interface FeatureFlags {
  flags: Record<string, boolean | undefined>
  variants: Record<string, { name: string; enabled?: boolean; payload?: { type: string; value: string } } | undefined>
}

let pending: Promise<FeatureFlags | null> | null = null
let loaded: FeatureFlags | null = null

export async function loadFeatureFlags(): Promise<FeatureFlags | null> {
  if (import.meta.env.MODE === 'test') return loaded
  pending ??= fetch(`https://feature-flags.${BASE_DOMAIN}/explorer.json`)
    .then(async (r) => (r.ok ? ((await r.json()) as FeatureFlags) : null))
    .catch(() => null)
    .then((f) => (loaded = f))
  return await pending
}

/** The flags once fetched; null while loading or when the service is unreachable. */
export function useFeatureFlags(): FeatureFlags | null {
  const [flags, setFlags] = useState(loaded)
  useEffect(() => {
    let live = true
    void loadFeatureFlags().then((f) => {
      if (live) setFlags(f)
    })
    return () => {
      live = false
    }
  }, [])
  return flags
}

export function flagEnabled(ff: FeatureFlags | null, name: string): boolean {
  return ff?.flags[`explorer-${name}`] === true
}

/** A flag's JSON payload when it is on and serving `variant`; undefined otherwise. */
export function flagPayload(ff: FeatureFlags | null, name: string, variant: string): unknown {
  const v = ff?.variants[`explorer-${name}`]
  if (!flagEnabled(ff, name) || v?.name !== variant || v.payload?.type !== 'json') return undefined
  try {
    return JSON.parse(v.payload.value)
  } catch {
    return undefined
  }
}

/** For tests. */
export function resetFeatureFlags(value: FeatureFlags | null = null): void {
  loaded = value
  pending = value != null ? Promise.resolve(value) : null
}
