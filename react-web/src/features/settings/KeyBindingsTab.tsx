// Key Bindings settings tab: every rebindable engine action with its current bindings as
// chips — click a chip to rebind it (press-a-key capture via the engine), ✕ removes it, [+]
// adds another binding. The whole edited table is sent through the bridge (setBindings is a
// whole-table replace engine-side) and the UI re-renders from the engine's echoed table, so
// what's shown is always what the engine actually holds.
//
// The directional families (movement, camera, pointer, scroll, zoom) are bindable per-action
// for keys/buttons, but an ANALOG capture (stick/wheel/trigger direction) also binds the
// action's opposite partner to the mirrored direction — see applyCapture. IaAny is not
// listed: it means "any scene button" and is handled in code, not a binding of its own.
import { useEffect } from 'react'
import { ModalShell, openPopup } from '../../design'
import type { ActionWire, BindingEntry, InputIdentifierWire } from '../../engine/protocol'
import {
  bindingsForAction,
  getBindingsSnapshot,
  labelForInput,
  sameAction,
  useBindingsSnapshot
} from '../../lib/bindingLabels'
import { isForwarded, markForwarded } from '../../lib/canvasForward'
import type { BindingsState } from '../session/useEngineSession'
import styles from './KeyBindingsTab.module.css'

// [wire action, friendly label]
const GAMEPLAY_ACTIONS: [ActionWire, string][] = [
  [{ Scene: 'IaForward' }, 'Move Forward'],
  [{ Scene: 'IaBackward' }, 'Move Backward'],
  [{ Scene: 'IaLeft' }, 'Move Left'],
  [{ Scene: 'IaRight' }, 'Move Right'],
  [{ Scene: 'IaJump' }, 'Jump'],
  [{ Scene: 'IaWalk' }, 'Walk'],
  [{ Scene: 'IaModifier' }, 'Sprint / Modifier'],
  [{ Scene: 'IaPointer' }, 'Pointer / Interact'],
  [{ Scene: 'IaPrimary' }, 'Primary Action'],
  [{ Scene: 'IaSecondary' }, 'Secondary Action'],
  [{ Scene: 'IaAction3' }, 'Action 3'],
  [{ Scene: 'IaAction4' }, 'Action 4'],
  [{ Scene: 'IaAction5' }, 'Action 5'],
  [{ Scene: 'IaAction6' }, 'Action 6'],
  [{ System: 'PointAt' }, 'Point At'],
  [{ System: 'Microphone' }, 'Microphone']
]

const INTERFACE_ACTIONS: [ActionWire, string][] = [
  [{ System: 'Map' }, 'Map'],
  [{ System: 'Places' }, 'Places'],
  [{ System: 'Communities' }, 'Communities'],
  [{ System: 'Backpack' }, 'Backpack'],
  [{ System: 'Gallery' }, 'Gallery'],
  [{ System: 'Settings' }, 'Settings'],
  [{ System: 'Friends' }, 'Friends'],
  [{ System: 'ChatPanel' }, 'Chat Panel'],
  [{ System: 'Chat' }, 'Focus Chat'],
  [{ System: 'ShowProfile' }, 'Show Profile'],
  [{ System: 'HideUi' }, 'Hide UI'],
  [{ System: 'HideNames' }, 'Hide Nametags'],
  [{ System: 'Cancel' }, 'Cancel']
]

const EMOTE_ACTIONS: [ActionWire, string][] = [
  [{ System: 'Emote' }, 'Emote Wheel'],
  ...Array.from({ length: 10 }, (_, slot): [ActionWire, string] => [
    { System: `QuickEmote${slot}` },
    `Quick Emote ${slot}`
  ])
]

const CAMERA_ACTIONS: [ActionWire, string][] = [
  [{ System: 'CameraLock' }, 'Camera Look'],
  [{ System: 'CameraUp' }, 'Camera Up'],
  [{ System: 'CameraDown' }, 'Camera Down'],
  [{ System: 'CameraLeft' }, 'Camera Left'],
  [{ System: 'CameraRight' }, 'Camera Right'],
  [{ System: 'CameraZoomIn' }, 'Camera Zoom In'],
  [{ System: 'CameraZoomOut' }, 'Camera Zoom Out'],
  [{ System: 'RollLeft' }, 'Camera Roll Left'],
  [{ System: 'RollRight' }, 'Camera Roll Right']
]

const POINTER_SCROLL_ACTIONS: [ActionWire, string][] = [
  [{ System: 'PointerUp' }, 'Pointer Up'],
  [{ System: 'PointerDown' }, 'Pointer Down'],
  [{ System: 'PointerLeft' }, 'Pointer Left'],
  [{ System: 'PointerRight' }, 'Pointer Right'],
  [{ System: 'ScrollUp' }, 'Scroll Up'],
  [{ System: 'ScrollDown' }, 'Scroll Down'],
  [{ System: 'ScrollLeft' }, 'Scroll Left'],
  [{ System: 'ScrollRight' }, 'Scroll Right']
]

const GROUPS: [string, [ActionWire, string][]][] = [
  ['Gameplay', GAMEPLAY_ACTIONS],
  ['Interface', INTERFACE_ACTIONS],
  ['Emotes', EMOTE_ACTIONS],
  ['Camera', CAMERA_ACTIONS],
  ['Pointer & Scroll', POINTER_SCROLL_ACTIONS]
]

const ALL_LISTED = GROUPS.flatMap(([, actions]) => actions)

type Direction = 'Up' | 'Down' | 'Left' | 'Right'

// Opposite-action pairs within the engine's directional families. An analog capture on one
// member also binds its partner to the opposite direction of the same axis (see
// applyCapture) — pairs only, never the whole 4-way set, so one stick can be split across
// families (vertical for look, horizontal for movement).
const OPPOSITE_PAIRS: [ActionWire, ActionWire][] = [
  [{ Scene: 'IaForward' }, { Scene: 'IaBackward' }],
  [{ Scene: 'IaLeft' }, { Scene: 'IaRight' }],
  [{ System: 'CameraUp' }, { System: 'CameraDown' }],
  [{ System: 'CameraLeft' }, { System: 'CameraRight' }],
  [{ System: 'PointerUp' }, { System: 'PointerDown' }],
  [{ System: 'PointerLeft' }, { System: 'PointerRight' }],
  [{ System: 'ScrollUp' }, { System: 'ScrollDown' }],
  [{ System: 'ScrollLeft' }, { System: 'ScrollRight' }],
  [{ System: 'CameraZoomIn' }, { System: 'CameraZoomOut' }],
  [{ System: 'RollLeft' }, { System: 'RollRight' }]
]

const OPPOSITE_DIR: Record<Direction, Direction> = { Up: 'Down', Down: 'Up', Left: 'Right', Right: 'Left' }

const isPair = (a: ActionWire, b: ActionWire): boolean =>
  OPPOSITE_PAIRS.some(
    (p) => (sameAction(p[0], a) && sameAction(p[1], b)) || (sameAction(p[1], a) && sameAction(p[0], b))
  )

// The camera-direction 4-set renders as one full-width box on its own row (still two
// independent opposite pairs for analog capture — the box is purely visual).
const CAMERA_QUAD = ['CameraUp', 'CameraDown', 'CameraLeft', 'CameraRight']

const isQuad = (actions: [ActionWire, string][], i: number): boolean =>
  CAMERA_QUAD.every((name, j) => {
    const a = actions[i + j]?.[0]
    return a != null && 'System' in a && a.System === name
  })

/** Group a section's rows so ADJACENT opposite-pair members render boxed as one unit. */
function chunkPairs(actions: [ActionWire, string][]): [ActionWire, string][][] {
  const chunks: [ActionWire, string][][] = []
  for (let i = 0; i < actions.length; i++) {
    if (isQuad(actions, i)) {
      chunks.push(actions.slice(i, i + 4))
      i += 3
    } else if (i + 1 < actions.length && isPair(actions[i][0], actions[i + 1][0])) {
      chunks.push([actions[i], actions[i + 1]])
      i++
    } else {
      chunks.push([actions[i]])
    }
  }
  return chunks
}

const ANALOG_WIRE = /^(MouseMove|MouseWheel|GamepadLeft|GamepadRight|GamepadLeftTrigger|GamepadRightTrigger) (Up|Down|Left|Right)$/

/** The table with `action`'s bindings replaced (appended if the action had no row). */
function withBinding(table: BindingEntry[], action: ActionWire, inputs: InputIdentifierWire[]): BindingEntry[] {
  if (!table.some(([a]) => sameAction(a, action))) return [...table, [action, inputs]]
  return table.map(([a, b]) => (sameAction(a, action) ? [a, inputs] : [a, b]))
}

const boundTo = (table: BindingEntry[], action: ActionWire): InputIdentifierWire[] =>
  table.find(([a]) => sameAction(a, action))?.[1] ?? []

/**
 * Apply a captured input to the table. A plain key/button replaces chip `index` of the one
 * action. An ANALOG capture (stick/wheel/trigger direction) on an opposite-pair member
 * binds the PUSHED direction to the captured action and the opposite direction to its
 * partner — so the push orients the axis: stick-up captured on Camera Up is normal look,
 * stick-up captured on Camera Down is inverted. Both pair members drop any previous binding
 * on the same axis first; bindings on other axes (e.g. wheel AND stick on scroll) and the
 * pair's perpendicular siblings are left alone.
 */
export function applyCapture(
  table: BindingEntry[],
  action: ActionWire,
  index: number,
  captured: InputIdentifierWire
): BindingEntry[] {
  // The wheel's scroll role is fixed: capturing a wheel direction onto a Scroll action
  // would only fight the engine re-adding the canonical binding — ignore it.
  if (isFixedBinding(action, captured)) return table
  const analog = ANALOG_WIRE.exec(captured)
  const pair = analog != null ? OPPOSITE_PAIRS.find((p) => p.some((a) => sameAction(a, action))) : undefined
  if (analog == null || pair == null) {
    const bound = [...boundTo(table, action)]
    bound[index] = captured
    // The same input twice on one action adds nothing — collapse duplicates.
    return withBinding(table, action, [...new Set(bound)])
  }
  const [axis, dir] = [analog[1], analog[2] as Direction]
  const partner = sameAction(pair[0], action) ? pair[1] : pair[0]
  const rebind = (t: BindingEntry[], a: ActionWire, d: Direction): BindingEntry[] =>
    withBinding(t, a, [...boundTo(t, a).filter((b) => !b.startsWith(`${axis} `)), `${axis} ${d}`])
  return rebind(rebind(table, action, dir), partner, OPPOSITE_DIR[dir])
}

// Scroll is a context action (only consumed over scrollable UI), so sharing its inputs with
// other actions is intentional — e.g. the wheel defaults deliberately overlap camera zoom.
const isScrollAction = (a: ActionWire): boolean => 'System' in a && a.System.startsWith('Scroll')

/** The wheel's scroll role is FIXED (engine-enforced): HUD and scene scrollables use native
 *  wheel scrolling, which can't follow rebinds, so MouseWheel entries on Scroll rows can be
 *  neither removed nor replaced. */
const isFixedBinding = (action: ActionWire, input: InputIdentifierWire): boolean =>
  isScrollAction(action) && input.startsWith('MouseWheel ')

/** Friendly names of OTHER listed actions also bound to `input` (the duplicate warning). */
function conflictsFor(table: BindingEntry[], action: ActionWire, input: InputIdentifierWire): string[] {
  if (isScrollAction(action)) return []
  return ALL_LISTED.filter(
    ([a]) =>
      !isScrollAction(a) &&
      !sameAction(a, action) &&
      (table.find(([b]) => sameAction(a, b))?.[1] ?? []).includes(input)
  ).map(([, label]) => label)
}

function BindingRow({
  action,
  label,
  onRebind,
  onRemove
}: {
  action: ActionWire
  label: string
  /** Rebind slot `index` (index === bound.length appends). */
  onRebind: (action: ActionWire, label: string, index: number) => void
  onRemove: (action: ActionWire, index: number) => void
}): React.JSX.Element {
  const snap = useBindingsSnapshot()
  const bound = bindingsForAction(snap, action)
  return (
    <div className={styles.row}>
      <span className={styles.rowLabel}>{label}</span>
      <span className={styles.chips}>
        {bound.map((input, i) => {
          if (isFixedBinding(action, input)) {
            return (
              <span key={`${input}${i}`} className={styles.chipWrap}>
                <span className={`${styles.chip} ${styles.chipFixed}`} title="The wheel always scrolls">
                  {labelForInput(snap, input)}
                </span>
              </span>
            )
          }
          const conflicts = conflictsFor(snap.bindings, action, input)
          return (
            <span key={`${input}${i}`} className={styles.chipWrap}>
              <button
                type="button"
                className={`${styles.chip} ${conflicts.length > 0 ? styles.chipConflict : ''}`.trim()}
                title={
                  conflicts.length > 0
                    ? `Also bound to ${conflicts.join(', ')} — click to rebind`
                    : 'Click to rebind'
                }
                onClick={() => onRebind(action, label, i)}
              >
                {labelForInput(snap, input)}
              </button>
              <button
                type="button"
                className={styles.chipRemove}
                aria-label={`Remove ${labelForInput(snap, input)} from ${label}`}
                onClick={() => onRemove(action, i)}
              >
                ×
              </button>
            </span>
          )
        })}
        <button
          type="button"
          className={styles.chipAdd}
          aria-label={`Add a binding for ${label}`}
          onClick={() => onRebind(action, label, bound.length)}
        >
          +
        </button>
      </span>
    </div>
  )
}

const MOUSE_WIRE = ['Mouse Left', 'Mouse Middle', 'Mouse Right', 'Mouse Back', 'Mouse Forward']

// The capture UI. Keyboard and gamepad resolve through the engine's GetNativeInput (keys
// reach the canvas via boot.js's forwarder; gamepads are polled directly). MOUSE resolves
// HERE, DOM-side (`onMouse`): on web winit's mouse listeners live ON the canvas, which this
// modal covers, so the engine cannot be relied on to see clicks. Pointer/wheel events are
// still cloned onto the canvas so the engine's pending capture is consumed and its input
// reservation released — on native (no canvas; the engine reads OS input directly) the
// engine resolves the same value itself, and the duplicate is dropped by request id.
//
// Every derived DOM event (click/auxclick/contextmenu) is suppressed so a click means
// "bind this button" — never close-the-popup or press-a-control; the resolve happens on
// pointerUP so both edges get forwarded and the engine never sees a stuck-down button.
// Forwarded clones are MARKED (canvasForward): a dispatched clone re-enters the same
// window/document capture path as real input, so unmarked forwarding recurses into these
// very handlers.
//
// Cancelling needs NO key handling here: keys flow to the engine and are captured like any
// other input (the stream is muted at BindInput, so nothing else reacts), and `rebind`
// interprets a Cancel-bound result as "abort" — see captureCancelInputs. That's also what
// lets a gamepad-bound Cancel abort a capture.
function CaptureModal({
  cancelHint,
  label,
  onMouse
}: {
  /** Label of the input that aborts the capture (absent when nothing does). */
  cancelHint?: string
  label: string
  onMouse: (input: InputIdentifierWire) => void
}): React.JSX.Element {
  useEffect(() => {
    const canvas = document.getElementById('mygame-canvas')
    const suppress = (e: Event): void => {
      e.preventDefault()
      e.stopPropagation()
    }
    let downButton: number | null = null
    const onPointer = (e: PointerEvent): void => {
      if (isForwarded(e)) return
      suppress(e)
      canvas?.dispatchEvent(markForwarded(new PointerEvent(e.type, e)))
      if (e.type === 'pointerdown') downButton = e.button
      else if (downButton === e.button) onMouse(MOUSE_WIRE[e.button] ?? 'Mouse Left')
    }
    const onWheel = (e: WheelEvent): void => {
      if (isForwarded(e)) return
      suppress(e)
      canvas?.dispatchEvent(markForwarded(new WheelEvent('wheel', e)))
      onMouse(
        Math.abs(e.deltaY) >= Math.abs(e.deltaX)
          ? e.deltaY < 0
            ? 'MouseWheel Up'
            : 'MouseWheel Down'
          : e.deltaX < 0
            ? 'MouseWheel Left'
            : 'MouseWheel Right'
      )
    }
    window.addEventListener('pointerdown', onPointer, true)
    window.addEventListener('pointerup', onPointer, true)
    window.addEventListener('wheel', onWheel, { capture: true, passive: false })
    window.addEventListener('click', suppress, true)
    window.addEventListener('auxclick', suppress, true)
    window.addEventListener('contextmenu', suppress, true)
    return () => {
      window.removeEventListener('pointerdown', onPointer, true)
      window.removeEventListener('pointerup', onPointer, true)
      window.removeEventListener('wheel', onWheel, { capture: true } as EventListenerOptions)
      window.removeEventListener('click', suppress, true)
      window.removeEventListener('auxclick', suppress, true)
      window.removeEventListener('contextmenu', suppress, true)
    }
  }, [onMouse])
  return (
    <ModalShell title={`Rebind ${label}`} width={400} closeButton={false} ariaLabel={`Rebind ${label}`}>
      <div className={styles.capturePrompt}>Press a key, mouse or gamepad button…</div>
      {cancelHint != null && <div className={styles.captureHint}>{cancelHint} to cancel</div>}
    </ModalShell>
  )
}

/** The inputs that ABORT a capture instead of binding: whatever Cancel is bound to (key,
 *  gamepad button — any type), or Escape while Cancel is unbound so the modal always stays
 *  escapable (the overlay suppresses clicks, so there is no backdrop dismiss). Empty when
 *  capturing for the Cancel action itself: everything must stay capturable there, or a
 *  cleared Escape could never be re-bound. */
function captureCancelInputs(action: ActionWire): InputIdentifierWire[] {
  if (sameAction(action, { System: 'Cancel' })) return []
  const bound = bindingsForAction(getBindingsSnapshot(), { System: 'Cancel' })
  return bound.length > 0 ? bound : ['Escape']
}

export function KeyBindingsTab({ bindings }: { bindings: BindingsState }): React.JSX.Element {
  const rebind = (action: ActionWire, label: string, index: number): void => {
    const { input, cancel } = bindings.capture()
    const cancels = captureCancelInputs(action)
    // Two resolution paths race — the engine capture (keyboard/gamepad, and mouse on
    // native) and the modal's DOM mouse handler — so the first result wins and the loser
    // is dropped (`cancel` also makes any late engine result stale by request id). A
    // Cancel-bound result means "abort": close without binding.
    let done = false
    const finish = (captured: string): void => {
      if (done) return
      done = true
      cancel()
      close()
      if (cancels.includes(captured)) return
      // Read the freshest table (the store may have updated while the modal was up).
      bindings.set(applyCapture(getBindingsSnapshot().bindings, action, index, captured))
    }
    const close = openPopup(
      () => (
        <CaptureModal
          label={label}
          cancelHint={cancels.length > 0 ? labelForInput(getBindingsSnapshot(), cancels[0]) : undefined}
          onMouse={finish}
        />
      ),
      {
        // closed some other way (resetPopups) → drop the pending capture's result
        onClose: () => {
          if (!done) {
            done = true
            cancel()
          }
        }
      }
    )
    void input.then(finish)
  }

  const remove = (action: ActionWire, index: number): void => {
    const table = getBindingsSnapshot().bindings
    const bound = boundTo(table, action).filter((_, i) => i !== index)
    bindings.set(withBinding(table, action, bound))
  }

  return (
    <div className={styles.root}>
      <p className={styles.note}>
        Bindings are stored by physical key position, so they keep working on any keyboard
        layout — the labels follow the keys on yours.
      </p>
      {GROUPS.map(([group, actions]) => (
        <section key={group}>
          <h2 className={styles.groupTitle}>{group}</h2>
          <div className={styles.rows}>
            {chunkPairs(actions).map((chunk) =>
              chunk.length > 1 ? (
                <div
                  key={chunk[0][1]}
                  className={chunk.length === 4 ? `${styles.pairBox} ${styles.quadBox}` : styles.pairBox}
                >
                  {chunk.map(([action, label]) => (
                    <BindingRow key={label} action={action} label={label} onRebind={rebind} onRemove={remove} />
                  ))}
                </div>
              ) : (
                <BindingRow key={chunk[0][1]} action={chunk[0][0]} label={chunk[0][1]} onRebind={rebind} onRemove={remove} />
              )
            )}
          </div>
        </section>
      ))}
    </div>
  )
}
