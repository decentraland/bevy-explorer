// Change keys for the pointer relays. Shared with the bridge scene, which sends only when a key
// changes, and with the page, which keeps its state when an update carries the same key.

import type { HoverAction, ProximityTip } from './protocol'

/** Tips at whole-pixel positions: sub-pixel camera jitter is not a change. */
export function proximityKey(tips: ProximityTip[]): string {
  return tips.map((t) => `${t.id}@${Math.round(t.x)},${Math.round(t.y)}:${hoverKey(t.actions)}`).join('|')
}

export function hoverKey(actions: HoverAction[]): string {
  return JSON.stringify(actions)
}
