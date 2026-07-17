// A key chord that fires no matter who holds keyboard focus.
//
// In the world the engine shares this document (its canvas is in the page — EngineHost "Approach A",
// no iframe), so a single capture-phase `window` listener sees the key wherever focus sits, ahead of
// the engine's own handlers.

import { useEffect, useRef } from 'react'

export function useGlobalHotkey(match: (e: KeyboardEvent) => boolean, onMatch: () => void): void {
  const matchRef = useRef(match)
  const onMatchRef = useRef(onMatch)
  matchRef.current = match
  onMatchRef.current = onMatch

  useEffect(() => {
    const handler = (e: KeyboardEvent): void => {
      if (!matchRef.current(e)) return
      e.preventDefault()
      onMatchRef.current()
    }
    window.addEventListener('keydown', handler, true)
    return () => window.removeEventListener('keydown', handler, true)
  }, [])
}
