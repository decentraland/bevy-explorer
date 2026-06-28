// HUD keyboard shortcuts (the [O]/[M]/[I]/[G]/[P]/[B]/[L]/[T]/[Z] hints shown in the nav + sidebar).
//
// These must fire even while playing: in the world the engine runs in a same-origin iframe that
// grabs keyboard focus, so its keydown events dispatch to the IFRAME window — a plain `window`
// listener never sees them. We attach to both the page window and the engine iframe window
// (polling until it mounts) in capture phase, mirroring useGlobalHotkey.

import { useEffect, useRef } from 'react'
import type { EngineSession } from '../features/session/useEngineSession'

// key → the session toggle it triggers. Keep in sync with the nav/sidebar `shortcut` hints.
const SHORTCUTS: Record<string, (s: EngineSession) => () => void> = {
  m: (s) => s.map.toggle,
  z: (s) => s.places.toggle, // Unity binds Places to Z
  o: (s) => s.communities.toggle,
  i: (s) => s.backpack.toggle,
  g: (s) => s.gallery.toggle,
  p: (s) => s.settings.toggle,
  b: (s) => s.emotes.toggle,
  l: (s) => s.friends.toggle,
  t: (s) => s.chat.toggle
}

export function useMenuShortcuts(session: EngineSession): void {
  const sessionRef = useRef(session)
  sessionRef.current = session

  useEffect(() => {
    const handler = (e: KeyboardEvent): void => {
      // Plain letters only — leave chords (Ctrl/Cmd/Alt) and keystrokes in text fields alone.
      if (e.ctrlKey || e.metaKey || e.altKey || e.repeat) return
      const target = e.target as HTMLElement | null
      const tag = target?.tagName
      if (tag === 'INPUT' || tag === 'TEXTAREA' || target?.isContentEditable) return

      const s = sessionRef.current
      if (s.phase !== 'world') return
      const toggle = SHORTCUTS[e.key.toLowerCase()]
      if (!toggle) return
      e.preventDefault()
      e.stopPropagation()
      toggle(s)()
    }

    const attached = new Set<Window>()
    const attach = (w: Window | null | undefined): void => {
      if (!w || attached.has(w)) return
      try {
        w.addEventListener('keydown', handler, true)
        attached.add(w)
      } catch {
        /* cross-origin window — ignore */
      }
    }
    attach(window)
    // The engine iframe mounts async; poll briefly to attach once it (re)appears.
    const poll = window.setInterval(() => {
      const iframe = document.querySelector<HTMLIFrameElement>('iframe[title="Decentraland engine"]')
      attach(iframe?.contentWindow)
    }, 500)

    return () => {
      window.clearInterval(poll)
      attached.forEach((w) => {
        try {
          w.removeEventListener('keydown', handler, true)
        } catch {
          /* window gone */
        }
      })
    }
  }, [])
}
