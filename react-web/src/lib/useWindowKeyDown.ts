// The one way to listen for a HUD hotkey.
//
// In the world the engine shares this document (its canvas is in the page — EngineHost "Approach A",
// no iframe) and grabs keyboard focus, so a hotkey has to be a `window` listener to see the key
// wherever focus sits. That's the mechanical part. The reason this is a shared wrapper rather than a
// raw `addEventListener` per feature is the gate below: a crash modal freezes HUD input (see
// inputLock), and every hotkey has to honour that. Routing them all through here means a new hotkey
// gets it for free instead of depending on whoever writes it remembering the check.
//
// Not for element-scoped keys (a `<input onKeyDown>` only fires when it has focus, and the crash
// modal's focus trap already takes focus away) — this is only for listeners on window/document.

import { useEffect, useRef } from 'react'
import { isInputLocked } from './inputLock'

export interface WindowKeyDownOptions {
  /** Capture phase — needed to beat the engine's own handlers on the shared window. Default true. */
  capture?: boolean
  /** Skip while false, without re-registering (the handler stays subscribed and no-ops). */
  enabled?: boolean
}

/**
 * Run `handler` on every keydown that reaches `window`, except while a crash modal holds HUD input.
 * `handler` is read from a ref, so it can close over fresh props without re-subscribing.
 */
export function useWindowKeyDown(handler: (e: KeyboardEvent) => void, options: WindowKeyDownOptions = {}): void {
  const { capture = true, enabled = true } = options
  const handlerRef = useRef(handler)
  handlerRef.current = handler
  const enabledRef = useRef(enabled)
  enabledRef.current = enabled

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent): void => {
      if (!enabledRef.current) return
      if (isInputLocked()) return // a crash modal owns the screen — nothing behind it may react
      handlerRef.current(e)
    }
    window.addEventListener('keydown', onKeyDown, capture)
    return () => window.removeEventListener('keydown', onKeyDown, capture)
  }, [capture])
}
