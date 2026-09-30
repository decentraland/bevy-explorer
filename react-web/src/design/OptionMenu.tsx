// OptionMenu — a light popover list of single-choice sections (e.g. SORT BY / VIEW), each option
// marked with a check when chosen. Closes on an outside click or Cancel.
import { useEffect, useRef } from 'react'
import { registerCancelLayer } from '../lib/cancelLayers'
import { MaskIcon } from './MaskIcon'
import checkIcon from '../assets/backpack/icon-check.webp'
import styles from './OptionMenu.module.css'

export interface OptionSection<T extends string = string> {
  label: string
  options: readonly { id: T; label: string }[]
  value: T
  onChange: (id: T) => void
}

export function OptionMenu({ sections, onClose, className }: { sections: readonly OptionSection[]; onClose: () => void; className?: string }): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const onDown = (e: MouseEvent): void => {
      const t = e.target as Node
      if (ref.current != null && !ref.current.contains(t) && !(ref.current.parentElement?.contains(t) ?? false)) onClose()
    }
    document.addEventListener('mousedown', onDown)
    const off = registerCancelLayer(onClose)
    return () => {
      document.removeEventListener('mousedown', onDown)
      off()
    }
  }, [onClose])
  return (
    <div ref={ref} className={`${styles.menu} ${className ?? ''}`.trim()} role="menu">
      {sections.map((s, i) => (
        <div key={s.label} className={styles.section}>
          {i > 0 && <span className={styles.separator} aria-hidden="true" />}
          <span className={styles.label}>{s.label}</span>
          {s.options.map((o) => (
            <button
              key={o.id}
              type="button"
              role="menuitemradio"
              aria-checked={o.id === s.value}
              className={styles.option}
              onClick={() => s.onChange(o.id)}
            >
              {o.label}
              {o.id === s.value && <MaskIcon src={checkIcon} size={14} className={styles.check} />}
            </button>
          ))}
        </div>
      ))}
    </div>
  )
}
