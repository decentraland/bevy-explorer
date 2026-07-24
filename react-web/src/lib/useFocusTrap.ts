// Trap keyboard focus inside an element while it's the active overlay. Used by PopupHost (the topmost
// popup) and the self-contained EngineErrorModal — anywhere that owns a modal surface and must keep
// Tab/Shift+Tab inside it. Focuses the element on activate, cycles at the ends, restores focus on close.
import { useEffect, type RefObject } from 'react'

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])'

/** While `active`, trap focus inside `ref`: focus it on activate, cycle Tab/Shift+Tab within it, and
 *  restore focus to the previously-focused element on deactivate. No-op while `ref` is unmounted. */
export function useFocusTrap(ref: RefObject<HTMLElement | null>, active: boolean): void {
  useEffect(() => {
    const root = ref.current
    if (!active || !root) return
    const prev = document.activeElement
    root.focus()
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== 'Tab') return
      const f = root.querySelectorAll<HTMLElement>(FOCUSABLE)
      if (!f.length) {
        e.preventDefault()
        root.focus()
        return
      }
      const first = f[0]
      const last = f[f.length - 1]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
    document.addEventListener('keydown', onKey, true)
    return () => {
      document.removeEventListener('keydown', onKey, true)
      if (prev instanceof HTMLElement) prev.focus()
    }
  }, [ref, active])
}
