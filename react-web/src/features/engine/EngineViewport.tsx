// A transparent placeholder that carves out a screen region for the engine to render
// into (the rich map, or the avatar preview). It measures its own on-screen rect and
// reports it to the scene over the bridge; the scene draws the engine view there, behind
// the React DOM, showing through this transparent hole. Clears the rect on unmount.

import { useLayoutEffect, useRef } from 'react'
import styles from './EngineViewport.module.css'

type Rect = { x: number; y: number; width: number; height: number }

export function EngineViewport({
  region,
  report
}: {
  region: 'map' | 'avatarPreview'
  /** Stable callback (useCallback) — reports the rect, or null to clear. */
  report: (region: 'map' | 'avatarPreview', rect: Rect | null) => void
}): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const push = (): void => {
      const r = el.getBoundingClientRect()
      report(region, { x: r.left, y: r.top, width: r.width, height: r.height })
    }
    push()
    const ro = new ResizeObserver(push)
    ro.observe(el)
    window.addEventListener('resize', push)
    window.addEventListener('scroll', push, true)
    return () => {
      ro.disconnect()
      window.removeEventListener('resize', push)
      window.removeEventListener('scroll', push, true)
      report(region, null)
    }
  }, [region, report])

  return <div ref={ref} className={styles.viewport} aria-hidden="true" />
}
