// HintsButton — a dark square button that toggles a popover of control hints above it (icon +
// text rows). Closes on an outside click or Cancel.
import { useEffect, useRef, useState } from 'react'
import { registerCancelLayer } from '../lib/cancelLayers'
import { MaskIcon } from './MaskIcon'
import keyboardIcon from '../assets/backpack/icon-keyboard.webp'
import styles from './HintsButton.module.css'

export interface Hint {
  icon: React.ReactNode
  text: string
}

export function HintsButton({ hints, label = 'Controls', className }: { hints: readonly Hint[]; label?: string; className?: string }): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const close = (): void => setOpen(false)
    const onDown = (e: MouseEvent): void => {
      if (ref.current != null && !ref.current.contains(e.target as Node)) close()
    }
    document.addEventListener('mousedown', onDown)
    const off = registerCancelLayer(close)
    return () => {
      document.removeEventListener('mousedown', onDown)
      off()
    }
  }, [open])
  return (
    <div ref={ref} className={`${styles.root} ${className ?? ''}`.trim()}>
      <button type="button" className={styles.button} aria-label={label} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <MaskIcon src={keyboardIcon} size={22} />
      </button>
      {open && (
        <ul className={styles.popover} aria-label={label}>
          {hints.map((h) => (
            <li key={h.text} className={styles.row}>
              <span className={styles.icon}>{h.icon}</span>
              {h.text}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** A key-cap outline with its label, for keyboard hints. */
export function KeyCap({ label }: { label: string }): React.JSX.Element {
  return <span className={styles.keyCap}>{label}</span>
}
