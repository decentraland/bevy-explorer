// The HUD's `--ui-scale`, and a signal for when it changes.
//
// Mirrors Unity's CanvasScaler ("Scale With Screen Size", 1080 reference height). `innerHeight`
// is logical px, so it is DPI-correct as-is: a Retina 3024x1890 framebuffer is ~945 logical tall
// → scale ~0.87.
//
// It is a store rather than a hook because the scale has a SUBSCRIBER that is not a React render:
// the engine cutouts (minimap, avatar preview) report their on-screen rect to the scene, and the
// HUD is scaled with a CSS transform — which changes no border box, so a ResizeObserver never
// fires for it. Those rects therefore have to be re-measured whenever the scale moves, and they
// have to be measured AFTER it has been applied. Both used to hang off `resize` listeners, and
// EngineViewport's was registered first (child effects run before parent effects), so every
// resize measured the rect at the old scale and nothing re-measured it — which is what left the
// engine drawing the minimap at its pre-resize size.
//
// So `apply` writes the custom property and THEN wakes subscribers, all in the same task: the
// subscriber's getBoundingClientRect() forces the style recalc, so it sees the new layout without
// deferring to a frame.

const REFERENCE_HEIGHT = 1080
const MIN = 0.6
const MAX = 1.3

let scale = 0
const listeners = new Set<() => void>()

function apply(): void {
  const next = Math.min(MAX, Math.max(MIN, window.innerHeight / REFERENCE_HEIGHT))
  if (next === scale) return
  scale = next
  document.documentElement.style.setProperty('--ui-scale', next.toFixed(3))
  listeners.forEach((l) => l())
}

/** Install the scale at boot (see main.tsx) and keep it in sync with the viewport. */
export function installHudScale(): void {
  apply()
  window.addEventListener('resize', apply)
}

export function getHudScale(): number {
  return scale
}

/** Wake `onChange` after `--ui-scale` has been written, so it can measure the scaled layout. */
export function subscribeHudScale(onChange: () => void): () => void {
  listeners.add(onChange)
  return () => { listeners.delete(onChange) }
}
