// Reticle + world-hover prompt. When the pointer is LOCKED (first-person / click-to-look) the
// browser hides the OS cursor, so we draw a center reticle ourselves; it highlights when the
// engine reports an interactable under it. The "press E to interact" prompt comes from the engine
// hover stream (relayed by the bridge's pointer domain) — InputAction → key label is mapped here.
import { useEffect, useState } from 'react'
import type { HoverAction } from '../../engine/protocol'
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

function KeyCap({ button }: { button: number }): React.JSX.Element {
  if (button === IA_POINTER) {
    return (
      <span className={styles.cap} aria-hidden="true">
        <svg width="12" height="17" viewBox="0 0 12 17">
          <rect x="0.7" y="0.7" width="10.6" height="15.6" rx="5.3" fill="none" stroke="currentColor" strokeWidth="1.2" />
          <line x1="6" y1="1" x2="6" y2="7" stroke="currentColor" strokeWidth="1.2" />
        </svg>
      </span>
    )
  }
  return <span className={styles.cap}>{KEY_LABEL[button] ?? '?'}</span>
}

export function Pointer({ hover }: { hover: HoverAction[] }): React.JSX.Element | null {
  const [locked, setLocked] = useState(false)
  useEffect(() => {
    const update = (): void => setLocked(document.pointerLockElement != null)
    update()
    document.addEventListener('pointerlockchange', update)
    return () => document.removeEventListener('pointerlockchange', update)
  }, [])

  const active = hover.length > 0
  if (!locked && !active) return null

  return (
    <div className={styles.root} aria-hidden="true">
      {locked && <div className={`${styles.reticle}${active ? ` ${styles.reticleActive}` : ''}`} />}
      {active && (
        <div className={styles.hints}>
          {hover.map((a, i) => (
            <div key={i} className={`${styles.hint}${a.enabled ? '' : ` ${styles.hintDisabled}`}`}>
              <KeyCap button={a.button} />
              <span className={styles.hintText}>{a.enabled ? a.text : 'Too far, get closer'}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
