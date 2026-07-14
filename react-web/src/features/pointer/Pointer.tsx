// Reticle + world-hover prompt. When the pointer is LOCKED (first-person / click-to-look) the
// browser hides the OS cursor, so we draw a center reticle ourselves; it highlights when the
// engine reports an interactable under it. The "press E to interact" prompt comes from the engine
// hover stream (relayed by the bridge's pointer domain) — InputAction → key label is mapped here.
import { useEffect, useState, useSyncExternalStore } from 'react'
import { CameraIcon, WalkIcon } from '../../design'
import type { HoverAction, ProximityTip } from '../../engine/protocol'
import { subscribeCursor, getCursor, setCursorNotify } from './cursorStore'
import styles from './Pointer.module.css'

// Default DCL key bindings (custom rebinds aren't reflected yet — v1).
const IA_POINTER = 0
const KEY_LABEL: Record<number, string> = {
  1: 'E', // IA_PRIMARY
  2: 'F', // IA_SECONDARY
  4: 'W',
  5: 'S',
  6: 'D',
  7: 'A',
  8: 'Space', // IA_JUMP
  9: 'Shift', // IA_WALK
  10: '1', // IA_ACTION_3
  11: '2',
  12: '3',
  13: '4'
}

// Left-click mouse glyph (Figma "Hover Tooltip" — left button filled orange). From the design system.
function MouseLeftIcon(): React.JSX.Element {
  return (
    <svg className={styles.mouse} width="22" height="22" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path fillRule="evenodd" clipRule="evenodd" d="M11.6746 11.8961V2.99524C11.6746 2.99524 4.33789 2.98667 4.33789 11.8961H11.6746Z" fill="#FF7439" />
      <rect x="4.50781" y="2.99524" width="15" height="20" rx="7.5" stroke="#ECEBED" strokeWidth="2" />
      <path d="M4.33789 11.9952H18.9999" stroke="#ECEBED" />
      <path d="M7.20715 0.590018C4.26466 1.54992 2.11196 4.23323 1.19016 6.9471" stroke="#ECEBED" strokeLinecap="round" />
    </svg>
  )
}

function KeyCap({ button }: { button: number }): React.JSX.Element {
  if (button === IA_POINTER) return <MouseLeftIcon />
  return <span className={styles.cap}>{KEY_LABEL[button] ?? '?'}</span>
}

// Radial tooltip slots around the free cursor — ported from bevy-ui-scene's hover-action-component:
// the first prompt sits to the RIGHT of the pointer, the second to the LEFT, then top-centre, then
// the lower and upper flanks. Up to 7 shown at once. Left-side slots reverse so the key-cap stays
// nearest the cursor. Offsets are pre-scale px (the container applies --ui-scale).
const H = 14 // horizontal gap from the cursor
const V = 34 // vertical spacing between rows
interface Slot { x: number; y: number; side: 'r' | 'l' | 't' }
const HOVER_SLOTS: Slot[] = [
  { x: H, y: 0, side: 'r' }, // right-middle
  { x: -H, y: 0, side: 'l' }, // left-middle
  { x: 0, y: -2 * V, side: 't' }, // top-centre
  { x: H, y: V, side: 'r' }, // right-lower
  { x: -H, y: V, side: 'l' }, // left-lower
  { x: H, y: -V, side: 'r' }, // right-upper
  { x: -H, y: -V, side: 'l' } // left-upper
]
function slotStyle(s: Slot): React.CSSProperties {
  if (s.side === 't') return { position: 'absolute', left: s.x, top: s.y, transform: 'translate(-50%, -50%)' }
  if (s.side === 'l') return { position: 'absolute', left: s.x, top: s.y, transform: 'translate(-100%, -50%)' }
  return { position: 'absolute', left: s.x, top: s.y, transform: 'translateY(-50%)' }
}

// Out-of-range hint: which rule gates it decides both the glyph and the copy. 'player' is the
// bevy-ui-scene case (its hover-actions showed a walking-person icon for the unreachable state);
// 'camera' — including entries with no distance rule at all, the implicit 10m default — is new here
// since the old scene never distinguished the two.
const TOO_FAR_COPY: Record<'camera' | 'player', { Icon: (p: { size: number; className?: string }) => React.JSX.Element; text: string }> = {
  camera: { Icon: CameraIcon, text: 'Get camera closer' },
  player: { Icon: WalkIcon, text: 'Get player closer' }
}

function Hint({ action, slot }: { action: HoverAction; slot?: Slot }): React.JSX.Element {
  const reverse = slot?.side === 'l'
  const tooFar = !action.enabled ? TOO_FAR_COPY[action.tooFarReason ?? 'camera'] : null
  return (
    <div
      className={`${styles.hint}${reverse ? ` ${styles.hintReverse}` : ''}${action.enabled ? '' : ` ${styles.hintDisabled}`}`}
      style={slot ? slotStyle(slot) : undefined}
    >
      {tooFar ? (
        <span className={styles.mouse} data-testid={`too-far-icon-${action.tooFarReason ?? 'camera'}`}>
          <tooFar.Icon size={22} />
        </span>
      ) : (
        <KeyCap button={action.button} />
      )}
      <span className={styles.hintText}>{action.enabled ? action.text : tooFar?.text}</span>
    </div>
  )
}

export function Pointer({
  hover,
  locked,
  proximity
}: {
  hover: HoverAction[]
  locked: boolean
  proximity: ProximityTip[]
}): React.JSX.Element | null {
  // Only re-renders while a free-cursor hover is active (see the cursor store above); the coords are
  // read straight from the DOM pointer, so nothing streams over the bridge.
  const cursorPos = useSyncExternalStore(subscribeCursor, getCursor)
  // The engine grabs the mouse without the browser Pointer Lock API, so `locked` comes from the
  // bridge. Keep the browser API as a fallback for setups that do use it.
  const [browserLocked, setBrowserLocked] = useState(false)
  useEffect(() => {
    const update = (): void => setBrowserLocked(document.pointerLockElement != null)
    update()
    document.addEventListener('pointerlockchange', update)
    return () => document.removeEventListener('pointerlockchange', update)
  }, [])

  const showReticle = locked || browserLocked
  const active = hover.length > 0
  // Wake the cursor store only when we're actually following the pointer (free cursor + a hover).
  useEffect(() => {
    setCursorNotify(active && !showReticle)
    return () => setCursorNotify(false)
  }, [active, showReticle])

  if (!showReticle && !active && proximity.length === 0) return null

  return (
    <div className={styles.root} aria-hidden="true">
      {showReticle && <div data-testid="reticle" className={`${styles.reticle}${active ? ` ${styles.reticleActive}` : ''}`} />}
      {active &&
        (showReticle ? (
          // Pointer-locked: stack the prompts just below the centre reticle.
          <div className={styles.hints}>
            {hover.map((a, i) => (
              <Hint key={i} action={a} />
            ))}
          </div>
        ) : (
          // Free cursor: spread the prompts radially around the pointer (up to 7 slots).
          <div className={styles.hintsRadial} style={{ left: cursorPos.x, top: cursorPos.y }}>
            {hover.slice(0, HOVER_SLOTS.length).map((a, i) => (
              <Hint key={i} action={a} slot={HOVER_SLOTS[i]} />
            ))}
          </div>
        ))}
      {/* In-range entity tooltips, anchored at the engine-projected screen coords. */}
      {proximity.map((tip) => (
        <div key={tip.id} data-testid={`proxtip-${tip.id}`} className={styles.proxTip} style={{ left: `${tip.x}px`, top: `${tip.y}px` }}>
          {tip.actions.map((a, i) => (
            <div key={i} className={styles.hint}>
              <KeyCap button={a.button} />
              <span className={styles.hintText}>{a.text}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  )
}
