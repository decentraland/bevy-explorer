// The screen area the persistent HUD occupies, measured live and reported to the engine as the
// interactable-area inset (scenes read it from UiCanvasInformation to keep their own UI clear).
//
// The persistent HUD (nav rail, minimap, chat, and the friends panel that takes the chat's dock)
// is docked to the left edge, so the inset is the rightmost edge any of those elements reaches;
// the other three sides are free. Each element registers its root with `hudInsetRef`, and the
// inset follows them as they mount, unmount and resize — whichever of the chat and the minimap
// is wider at the time sets it.
//
// Measuring is deferred to a microtask: it coalesces a burst of signals into one read, and it
// runs after every effect of the commit that raised it, so a full-screen page that unmounts the
// HUD has already switched reporting off (see `useHudInsetReport`) by the time it is measured.

import { useLayoutEffect } from 'react'
import { subscribeHudScale } from './hudScale'

/** Insets from each window edge, in CSS pixels. */
export interface InteractableArea {
  left: number
  top: number
  right: number
  bottom: number
}

const elements = new Set<Element>()
let reporter: ((area: InteractableArea) => void) | null = null
let reported: number | null = null
let queued = false
let observer: ResizeObserver | null = null

function measure(): void {
  queued = false
  if (reporter == null) return
  let left = 0
  for (const el of elements) {
    const r = el.getBoundingClientRect()
    if (r.width > 0 && r.height > 0) left = Math.max(left, r.right)
  }
  left = Math.round(left)
  if (left === reported) return
  reported = left
  reporter({ left, top: 0, right: 0, bottom: 0 })
}

function schedule(): void {
  if (queued) return
  queued = true
  queueMicrotask(measure)
}

/** Ref callback for the root of a persistent HUD element: counts it towards the inset while it is
 *  mounted. */
export function hudInsetRef(el: Element | null): (() => void) | undefined {
  if (el == null) return
  observer ??= new ResizeObserver(schedule)
  observer.observe(el)
  // A slide (the rail's auto-hide) moves the rect without resizing it.
  el.addEventListener('transitionend', schedule)
  elements.add(el)
  schedule()
  return () => {
    observer?.unobserve(el)
    el.removeEventListener('transitionend', schedule)
    elements.delete(el)
    schedule()
  }
}

/** Report the inset through `report` (stable callback) whenever it changes while `enabled`.
 *  Disabled, nothing is sent and the engine keeps the last value. */
export function useHudInsetReport(report: (area: InteractableArea) => void, enabled: boolean): void {
  useLayoutEffect(() => {
    if (!enabled) return
    reporter = report
    schedule()
    // The HUD scales with a CSS transform, which changes no border box (see hudScale.ts).
    const unsubscribe = subscribeHudScale(schedule)
    return () => {
      reporter = null
      reported = null
      unsubscribe()
    }
  }, [report, enabled])
}
