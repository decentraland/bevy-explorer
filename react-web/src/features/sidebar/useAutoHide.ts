// Sidebar auto-hide — unity-explorer MainUIController: the rail slides out 0.3s after the pointer
// leaves and back 0.3s after it reaches the screen edge. Stored locally because the rail unmounts
// while a full-screen page is open.

import { useCallback, useEffect, useRef, useState } from 'react'

const STORAGE_KEY = 'hud.sidebarAutoHide'
export const AUTO_HIDE_DELAY_MS = 300

function readStored(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1'
  } catch {
    return false
  }
}

export interface AutoHide {
  enabled: boolean
  setEnabled: (on: boolean) => void
  hidden: boolean
  onPointerEnter: () => void
  onPointerLeave: () => void
}

/** `hold` keeps the rail out while something anchored to it (a popover) is open. */
export function useAutoHide(hold: boolean): AutoHide {
  const [enabled, setEnabledState] = useState(readStored)
  const [hidden, setHidden] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const schedule = useCallback((next: boolean) => {
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => setHidden(next), AUTO_HIDE_DELAY_MS)
  }, [])

  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current)
  }, [])

  const setEnabled = useCallback((on: boolean) => {
    setEnabledState(on)
    try {
      localStorage.setItem(STORAGE_KEY, on ? '1' : '0')
    } catch {
      /* storage blocked: the choice lasts this session only */
    }
    if (!on) {
      if (timer.current) clearTimeout(timer.current)
      setHidden(false)
    }
  }, [])

  return {
    enabled,
    setEnabled,
    hidden: enabled && hidden && !hold,
    onPointerEnter: () => enabled && schedule(false),
    onPointerLeave: () => enabled && !hold && schedule(true)
  }
}
