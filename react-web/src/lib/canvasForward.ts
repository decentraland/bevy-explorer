// Marker for events the HUD re-dispatches onto the engine canvas (winit's web listeners are
// attached to the canvas element, so events swallowed by a HUD overlay must be cloned onto it
// for the engine to see them — see CaptureModal, the one remaining forwarder). A dispatched
// clone propagates through the same window/document capture-and-bubble path as real input,
// so without the marker the forwarding handler re-enters itself, endlessly. Handlers that
// react to global input check isForwarded() and ignore marked clones; the engine (which
// doesn't know the marker) still receives them.

const MARK = '__hudForwardedToCanvas'

/** Tag a clone as HUD-forwarded before dispatching it on the canvas. */
export function markForwarded<T extends Event>(e: T): T {
  ;(e as unknown as Record<string, boolean>)[MARK] = true
  return e
}

/** Was this event forwarded by the HUD (as opposed to real user input)? */
export function isForwarded(e: Event): boolean {
  return (e as unknown as Record<string, unknown>)[MARK] === true
}
