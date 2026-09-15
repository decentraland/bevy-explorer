// Guard against accidentally leaving the world. The macOS two-finger swipe-back gesture (and the
// browser Back button) is a history "back" navigation that would unload the engine instantly. While
// `active`, we keep a sentinel history entry in front of the app so a back lands on us (firing
// popstate) instead of leaving — and surface a confirm modal. Refresh, tab-close and other navigations
// are deliberately left unchallenged.

import { useCallback, useEffect, useRef, useState } from 'react'

export interface ExitGuard {
  /** True while the leave-confirmation modal should be shown. */
  confirming: boolean
  /** Dismiss the modal and stay in the world. */
  stay: () => void
  /** Proceed with the back navigation the gesture intended (unloads the engine). */
  leave: () => void
}

export function useExitGuard(active: boolean): ExitGuard {
  const [confirming, setConfirming] = useState(false)
  // Set just before an intentional leave so the popstate handler doesn't re-trap on the way out.
  const leavingRef = useRef(false)

  useEffect(() => {
    if (!active) return
    leavingRef.current = false

    const onPopState = (): void => {
      if (leavingRef.current) return
      // The back consumed our sentinel — we're still on the page. Re-arm a sentinel so a second back
      // is trapped too, then ask.
      window.history.pushState(null, '', window.location.href)
      setConfirming(true)
    }

    window.history.pushState(null, '', window.location.href) // sentinel: catch the first back
    window.addEventListener('popstate', onPopState)
    return () => {
      window.removeEventListener('popstate', onPopState)
    }
  }, [active])

  // Stable identities: App opens the confirm popup from an effect keyed on these; a fresh callback each
  // render would close and reopen the popup under the user.
  const stay = useCallback((): void => setConfirming(false), [])
  const leave = useCallback((): void => {
    setConfirming(false)
    leavingRef.current = true
    // Go back past the two sentinels (setup + the one re-armed on the trapped back) and the app entry,
    // landing on wherever the user came from. No-op if the app is the first history entry.
    window.history.go(-2)
  }, [])

  return { confirming, stay, leave }
}
