// Guard against accidentally leaving the world. The macOS two-finger swipe-back gesture (and the
// browser Back button) is a history "back" navigation that would unload the engine instantly. While
// `active`, we keep a sentinel history entry in front of the app so a back lands on us (firing
// popstate) instead of leaving — and surface a confirm modal. Refresh / tab-close fall back to the
// browser's native "Leave site?" prompt (its text can't be customised).

import { useEffect, useRef, useState } from 'react'

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
  // Set just before an intentional leave so the handlers don't re-trap / double-prompt on the way out.
  const leavingRef = useRef(false)

  useEffect(() => {
    if (!active) return
    leavingRef.current = false

    const onBeforeUnload = (e: BeforeUnloadEvent): void => {
      if (leavingRef.current) return
      e.preventDefault()
      e.returnValue = '' // some browsers require a set returnValue to show the prompt
    }
    const onPopState = (): void => {
      if (leavingRef.current) return
      // The back consumed our sentinel — we're still on the page. Re-arm a sentinel so a second back
      // is trapped too, then ask.
      window.history.pushState(null, '', window.location.href)
      setConfirming(true)
    }

    window.history.pushState(null, '', window.location.href) // sentinel: catch the first back
    window.addEventListener('beforeunload', onBeforeUnload)
    window.addEventListener('popstate', onPopState)
    return () => {
      window.removeEventListener('beforeunload', onBeforeUnload)
      window.removeEventListener('popstate', onPopState)
    }
  }, [active])

  const stay = (): void => setConfirming(false)
  const leave = (): void => {
    setConfirming(false)
    leavingRef.current = true
    // Go back past the two sentinels (setup + the one re-armed on the trapped back) and the app entry,
    // landing on wherever the user came from. No-op if the app is the first history entry.
    window.history.go(-2)
  }

  return { confirming, stay, leave }
}
