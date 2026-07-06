// A key chord that fires no matter who holds keyboard focus.
//
// In the world the engine runs in a same-origin iframe that grabs focus, so its keydown
// events dispatch to the IFRAME's window — a plain `window` listener never sees them.
// We attach to both the page window and the engine iframe window (polling until it mounts),
// in capture phase so the engine's own handlers can't swallow the key first.

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
