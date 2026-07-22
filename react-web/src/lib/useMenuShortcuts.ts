// HUD keyboard shortcuts (the [O]/[M]/[I]/[G]/[P]/[B]/[L]/[T]/[Z] hints shown in the nav + sidebar).
//
// These must fire even while playing: in the world the engine shares this document (its canvas is in
// the page — EngineHost "Approach A", no iframe) and grabs keyboard focus, so we attach one
// capture-phase `window` listener that sees the key wherever focus sits, mirroring useGlobalHotkey.

import { useEffect, useRef } from 'react'
import type { EngineSession } from '../features/session/useEngineSession'

// Set by the wasm via boot.js __setEngineTextFocus: true while an engine-rendered text field
// (e.g. a scene textinput) holds keyboard focus. Those fields live on the canvas, so the
// e.target tag check below can't see them.
type EngineFocusWindow = Window & { __engineTextFocus?: boolean }

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
  t: (s) => s.chat.toggle,
  // Enter focuses chat (DCL convention) even when DOM focus sits on some other HUD control —
  // otherwise the browser would just "activate" that focused element (e.g. click a button).
  enter: (s) => s.chat.requestFocus
}

export function useMenuShortcuts(session: EngineSession): void {
  const sessionRef = useRef(session)
  sessionRef.current = session

  useEffect(() => {
    const keyDownHandler = (e: KeyboardEvent): void => {
      // Plain keys only — leave chords (Ctrl/Cmd/Alt) and keystrokes in text fields alone.
      if (e.ctrlKey || e.metaKey || e.altKey || e.repeat) return
      const target = e.target as HTMLElement | null
      const tag = target?.tagName
      if (tag === 'INPUT' || tag === 'TEXTAREA' || target?.isContentEditable) return
      if ((window as EngineFocusWindow).__engineTextFocus) return

      const s = sessionRef.current
      if (s.phase !== 'world') return

      // Quick emotes: while the wheel is open, a number key (0–9) plays that slot's emote (which also
      // closes the wheel). With the wheel closed a number does nothing — this covers both "hold B, tap
      // a number" and "tap B, then a number", since either way B has opened the wheel first. Handled
      // here (not off the engine's QuickEmote action) so it fires while the HUD holds keyboard focus.
      if (s.emotes.open && /^[0-9]$/.test(e.key)) {
        e.preventDefault()
        e.stopPropagation()
        const emote = s.emotes.list.find((em) => em.slot === Number(e.key))
        if (emote) s.emotes.play(emote.urn)
        return
      }

      const toggle = SHORTCUTS[e.key.toLowerCase()]
      if (!toggle) return
      e.preventDefault()
      e.stopPropagation()
      toggle(s)()
    }

    // Same-document engine (no iframe): the canvas shares this window, so one capture-phase
    // listener sees keys wherever focus is.
    window.addEventListener('keydown', keyDownHandler, true)
    return () => window.removeEventListener('keydown', keyDownHandler, true)
  }, [])
}
