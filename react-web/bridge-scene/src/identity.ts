// The scene outlives logout, so per-account caches register a reset for when another account logs in.

export type IdentityHub = {
  onChange: (reset: () => void) => void
  /** Feed the current address; resets run when it differs from the last one seen. */
  observe: (address: string | null) => void
}

export function createIdentityHub(): IdentityHub {
  const resets = new Set<() => void>()
  let last: string | null | undefined
  return {
    onChange: (reset) => {
      resets.add(reset)
    },
    observe: (address) => {
      if (last === undefined || address === null) {
        if (address !== null) last = address
        return
      }
      if (address === last) return
      last = address
      for (const reset of resets) {
        try {
          reset()
        } catch (e) {
          console.error('[identity] reset failed', e)
        }
      }
    }
  }
}

export const identity = createIdentityHub()
