// Shared cursor-position store. The engine canvas shares this document (no iframe), so a single
// window `mousemove` gives us the pointer directly — the bridge no longer streams it. The listener is
// always-on but near-free: it only records the coords. It wakes subscribers (<Pointer>) ONLY while a
// hover is active (`hoverActive`), so idle mouse movement never triggers a re-render. Because the
// coords stay current even while idle, a consumer reading getCursor() at any moment — e.g. the instant
// a nearby-avatar click arrives, to anchor the profile card — gets the live pointer position.
let cursor = { x: 0, y: 0 }
let hoverActive = false
const listeners = new Set<() => void>()

if (typeof window !== 'undefined') {
  window.addEventListener('mousemove', (e) => {
    cursor = { x: e.clientX, y: e.clientY }
    if (hoverActive) listeners.forEach((l) => l())
  })
}

export function subscribeCursor(onChange: () => void): () => void {
  listeners.add(onChange)
  return () => { listeners.delete(onChange) }
}

export function getCursor(): { x: number; y: number } {
  return cursor
}

/** <Pointer> toggles this so the store only re-renders it while a free-cursor hover is on screen. */
export function setCursorNotify(active: boolean): void {
  hoverActive = active
}
