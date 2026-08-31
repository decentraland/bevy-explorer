// Live key-binding labels for the HUD's shortcut hints (sidebar/menu "[M]", the world hover
// keycaps, the Key Bindings settings tab).
//
// A module external store (the popups/cursorStore pattern) rather than session prop-drilling:
// the labels are needed by leaf components (MainMenuShell, Pointer) that don't receive the
// session, and by design every consumer should show the same table. useEngineSession is the
// single writer — it mirrors each incoming `bindings` message here.
//
// Engine KeyCodes are PHYSICAL positions (w3c KeyboardEvent.code): on an AZERTY keyboard the
// key labeled Z reports KeyW, so movement "WASD" is already physically ZQSD there and needs no
// remapping — only the DISPLAYED labels differ per layout. navigator.keyboard.getLayoutMap()
// (Chromium — covers web Chrome and native CEF) gives the truthful per-layout character; the
// prettified code name is the fallback elsewhere.

import { useSyncExternalStore } from 'react'
import type { ActionWire, BindingEntry, InputIdentifierWire } from '../engine/protocol'

export interface BindingsSnapshot {
  bindings: BindingEntry[]
  /** KeyboardEvent.code → the character that key produces in the viewer's layout, or null
   *  until (unless) getLayoutMap resolves. Covers writing-system keys only. */
  layout: Map<string, string> | null
}

let snapshot: BindingsSnapshot = { bindings: [], layout: null }
const listeners = new Set<() => void>()
const emit = (): void => listeners.forEach((l) => l())

/** Mirror the engine's binding table (called by useEngineSession on every `bindings` message). */
export function setBindingsSnapshot(bindings: BindingEntry[]): void {
  snapshot = { ...snapshot, bindings }
  emit()
}

type KeyboardNavigator = Navigator & {
  keyboard?: { getLayoutMap?: () => Promise<Iterable<[string, string]>> }
}
void (navigator as KeyboardNavigator).keyboard
  ?.getLayoutMap?.()
  .then((m) => {
    snapshot = { ...snapshot, layout: new Map(m) }
    emit()
  })
  .catch(() => {}) // permissions-policy or unsupported → fallback labels

const subscribe = (cb: () => void): (() => void) => {
  listeners.add(cb)
  return () => listeners.delete(cb)
}
const getSnapshot = (): BindingsSnapshot => snapshot

/** The live binding table + layout map; re-renders on updates. */
export function useBindingsSnapshot(): BindingsSnapshot {
  return useSyncExternalStore(subscribe, getSnapshot)
}

const MOUSE_LABELS: Record<string, string> = { Left: 'LMB', Right: 'RMB', Middle: 'MMB' }
const AXIS_LABELS: Record<string, string> = {
  MouseMove: 'Mouse',
  MouseWheel: 'Wheel',
  GamepadLeft: 'L-Stick',
  GamepadRight: 'R-Stick',
  GamepadLeftTrigger: 'LT',
  GamepadRightTrigger: 'RT'
}
const KEY_LABELS: Record<string, string> = {
  Escape: 'Esc',
  ShiftLeft: 'Shift',
  ShiftRight: 'R-Shift',
  ControlLeft: 'Ctrl',
  ControlRight: 'R-Ctrl',
  AltLeft: 'Alt',
  AltRight: 'R-Alt',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→'
}

/** Human label for an InputIdentifier wire string ('KeyZ' | 'Mouse Left' | 'Gamepad South' |
 *  'MouseWheel Down' …), layout-aware for plain keys. */
export function labelForInput(snap: BindingsSnapshot, input: InputIdentifierWire): string {
  const mouse = input.match(/^Mouse (.+)$/)
  if (mouse) return MOUSE_LABELS[mouse[1]] ?? `Mouse ${mouse[1]}`
  const gamepad = input.match(/^Gamepad (.+)$/)
  if (gamepad) return `Pad ${gamepad[1]}`
  const analog = input.match(/^(\w+) (Up|Down|Left|Right)$/)
  if (analog && AXIS_LABELS[analog[1]]) return `${AXIS_LABELS[analog[1]]} ${analog[2]}`
  // A plain KeyCode. The layout map speaks for writing-system keys; everything else prettifies
  // the code name (strip Key/Digit prefixes, split camelCase, shorten modifiers/arrows).
  const fromLayout = snap.layout?.get(input)
  if (fromLayout) return fromLayout.length === 1 ? fromLayout.toUpperCase() : fromLayout
  if (KEY_LABELS[input]) return KEY_LABELS[input]
  const numpad = input.match(/^Numpad(.+)$/)
  if (numpad) return `Num ${numpad[1]}`
  const stripped = input.replace(/^Key(?=[A-Z0-9]$)|^Digit(?=\d$)/, '')
  return stripped.replace(/([a-z0-9])([A-Z])/g, '$1 $2')
}

/** The current snapshot, for imperative reads (event handlers applying a rebind). */
export function getBindingsSnapshot(): BindingsSnapshot {
  return snapshot
}

export const sameAction = (a: ActionWire, b: ActionWire): boolean =>
  ('System' in a && 'System' in b && a.System === b.System) ||
  ('Scene' in a && 'Scene' in b && a.Scene === b.Scene)

/** All identifiers bound to an action (empty until the table arrives / if unbound). */
export function bindingsForAction(snap: BindingsSnapshot, action: ActionWire): InputIdentifierWire[] {
  return snap.bindings.find(([a]) => sameAction(a, action))?.[1] ?? []
}

/** Is this wire string a plain keyboard KeyCode (vs mouse/gamepad/axis-direction)? */
const isKeyWire = (input: InputIdentifierWire): boolean =>
  !/^(Mouse|Gamepad)\s|\s(Up|Down|Left|Right)$/.test(input)

/** The label of an action's first KEY binding — the sidebar/menu "[M]"-style hint. Undefined
 *  when unbound or bound only to mouse/gamepad (no hint shown, matching the old hardcoded
 *  letters which were keyboard-only). */
export function keyHintFor(snap: BindingsSnapshot, systemAction: string): string | undefined {
  const key = bindingsForAction(snap, { System: systemAction }).find(isKeyWire)
  return key != null ? labelForInput(snap, key) : undefined
}

/** The key codes that cancel/close HUD UI — the Cancel action's key bindings. Until the
 *  engine's table arrives there is no truth to follow, so Escape stands in (pre-world popups
 *  stay dismissible); once the table is here it is authoritative — an unbound Cancel means NO
 *  key cancels, matching the engine, which resolves nothing. */
export function cancelKeyCodes(): string[] {
  if (snapshot.bindings.length === 0) return ['Escape']
  return bindingsForAction(snapshot, { System: 'Cancel' }).filter(isKeyWire)
}

/** Is this a text-entry element (keystrokes there are typing, not hotkeys/cancel)? */
export const isEditableTarget = (t: EventTarget | null | undefined): boolean =>
  t instanceof HTMLElement && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)

/** Does this keydown mean cancel? Matches the physical `code`; the `key` fallback keeps
 *  synthetic events that only set `key` (tests) working while Escape is the cancel key. */
export function isCancelKey(e: Pick<KeyboardEvent, 'key' | 'code'> & { target?: EventTarget | null }): boolean {
  // Inside a text field a printable key is typing, never cancel — only a non-printing
  // cancel key (Escape, an F-key…) may close/blur from within one.
  if (e.key.length === 1 && isEditableTarget(e.target)) return false
  const codes = cancelKeyCodes()
  return codes.includes(e.code) || (e.key === 'Escape' && codes.includes('Escape'))
}
