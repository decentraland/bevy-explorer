// Explicit cancel layering for leaf UI (the gallery lightbox, an open dropdown): a layer
// registers while it is open and the session's Cancel dispatch asks the TOPMOST layer first,
// stopping there — one layer per press, exactly like the popup/panel steps below it.
//
// This replaces DOM-side interception (catch the cancel key, stopPropagation so the engine
// never resolves it): the key now flows to the engine like any other input and the layering
// is enforced by dispatch order instead of event suppression — which is what lets a GAMEPAD
// Cancel (never a DOM event) peel these layers too. Registration order is the stack order,
// so nested layers must mount innermost-last (they do: a layer only exists while open).
//
// Popups are dispatched BEFORE these layers (see useEngineSession): every current layer sits
// under any popup that can appear over it (profile card over the lightbox). If a layer ever
// lives inside a popup (a dropdown in a modal), the two stacks need merging by recency.

const layers: (() => void)[] = []

/** Register while open/mounted; returns the unregister. The handler closes the layer. */
export function registerCancelLayer(onCancel: () => void): () => void {
  layers.push(onCancel)
  return () => {
    const i = layers.lastIndexOf(onCancel)
    if (i >= 0) layers.splice(i, 1)
  }
}

/** Close the topmost layer. Returns false if none is open (dispatch falls through to panels). */
export function dispatchCancelLayer(): boolean {
  const top = layers[layers.length - 1]
  if (top == null) return false
  top()
  return true
}
