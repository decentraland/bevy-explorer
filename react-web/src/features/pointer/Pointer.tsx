// Reticle + world-hover prompt. When the pointer is LOCKED (first-person / click-to-look) the
// browser hides the OS cursor, so we draw a center reticle ourselves; it highlights when the
// engine reports an interactable under it. The "press E to interact" prompt comes from the engine
// hover stream (relayed by the bridge's pointer domain) — InputAction → key label is mapped here.
import { useEffect, useState } from 'react'
import type { HoverAction, ProximityTip } from '../../engine/protocol'
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

export function Pointer({ hover, hoverPos, locked, proximity }: { hover: HoverAction[]; hoverPos?: { x: number; y: number } | null; locked: boolean; proximity: ProximityTip[] }): React.JSX.Element | null {
  // The engine grabs the mouse without the browser Pointer Lock API, so `locked` comes from the
  // bridge (PrimaryPointerInfo). Keep the browser API as a fallback for setups that do use it.
  const [browserLocked, setBrowserLocked] = useState(false)
  useEffect(() => {
    const update = (): void => setBrowserLocked(document.pointerLockElement != null)
    update()
    document.addEventListener('pointerlockchange', update)
    return () => document.removeEventListener('pointerlockchange', update)
  }, [])

  const showReticle = locked || browserLocked
  const active = hover.length > 0
  if (!showReticle && !active && proximity.length === 0) return null

  return (
    <div className={styles.root} aria-hidden="true">
      {showReticle && <div data-testid="reticle" className={`${styles.reticle}${active ? ` ${styles.reticleActive}` : ''}`} />}
      {active && (
        // Anchor the hint at the cursor (from PrimaryPointerInfo) when the pointer is free; fall back
        // to below the reticle when pointer-locked or the position is unavailable.
        <div className={styles.hints} style={!showReticle && hoverPos ? { left: hoverPos.x, top: hoverPos.y + 18 } : undefined}>
          {hover.map((a, i) => (
            <div key={i} className={`${styles.hint}${a.enabled ? '' : ` ${styles.hintDisabled}`}`}>
              <KeyCap button={a.button} />
              <span className={styles.hintText}>{a.enabled ? a.text : 'Too far, get closer'}</span>
            </div>
          ))}
        </div>
      )}
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
