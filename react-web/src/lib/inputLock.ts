// HUD-wide input lock: while a crash modal owns the screen, every hotkey stands down.
//
// A crash screen is a diagnostic surface — what's behind it must stay exactly as the user left it, so
// a screenshot still shows the popup or panel that was open when the engine blew up. Nothing may open,
// close or move under the scrim, and the popup layer's Escape must not silently fire an onClose
// contract the user can't see. (A world-not-found is NOT a crash and never takes the lock — it's an
// ordinary popup, see RealmErrorModal.)
//
// A flag rather than a swallow-everything capture listener: the crash modal mounts long after the
// HUD's hotkey hooks, so its listener on the same node and phase would run *after* theirs
// (stopImmediatePropagation only beats listeners registered later on that node), and a blanket
// swallow would also kill the modal's own focus trap, stranding keyboard users away from
// Dismiss/Reload.
//
// Rather than leave every handler to remember the check, `useWindowKeyDown` is the single wrapper
// around a global keydown listener and applies the gate for them. Only two places read the flag
// directly: the popup layer's Escape/focus-trap coordination in popups.tsx, and useFocusTrap — which
// can't go through the wrapper, since the crash modal itself needs Tab to keep working.

let holders = 0
const listeners = new Set<() => void>()
const emit = (): void => listeners.forEach((l) => l())

/** Is a fatal overlay holding input? Read at call time inside key handlers — the flag flips without
 *  a React render. */
export function isInputLocked(): boolean {
  return holders > 0
}

/** Take the lock; returns the release handle for the effect cleanup. Counted, so the two places App
 *  can render a crash modal from (session fatalError, ErrorBoundary fallback) never leave it stuck. */
export function lockInput(): () => void {
  holders++
  emit()
  let released = false
  return () => {
    if (released) return
    released = true
    holders--
    emit()
  }
}

/** Subscribe for React consumers that must re-render when the lock flips (useSyncExternalStore). */
export function subscribeInputLock(cb: () => void): () => void {
  listeners.add(cb)
  return () => listeners.delete(cb)
}
