// Select — custom dropdown (field + popup list) for full Figma-styled control,
// used by Settings (resolution, quality, …) and anywhere a choice is needed.

import { useEffect, useRef, useState } from 'react'
import styles from './Select.module.css'

export interface SelectOption {
  value: string
  label: string
}

interface SelectProps {
  value: string
  options: SelectOption[]
  onChange: (value: string) => void
  disabled?: boolean
  /** dark (default, on dark panels) or light (white field, e.g. Settings). */
  variant?: 'dark' | 'light'
  'aria-label'?: string
}

export function Select({
  value,
  options,
  onChange,
  disabled = false,
  variant = 'dark',
  'aria-label': ariaLabel
}: SelectProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDoc = (e: MouseEvent): void => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDoc)
    return () => document.removeEventListener('mousedown', onDoc)
  }, [open])

  const current = options.find((o) => o.value === value)

  return (
    <div className={styles.root} ref={ref}>
      <button
        type="button"
        className={`${styles.field} ${variant === 'light' ? styles.light : ''}`.trim()}
        disabled={disabled}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className={styles.value}>{current?.label ?? value}</span>
        <svg className={`${styles.chev} ${open ? styles.chevOpen : ''}`.trim()} viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2.5 4.5L6 8l3.5-3.5" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <ul className={styles.list} role="listbox">
          {options.map((o) => (
            <li key={o.value}>
              <button
                type="button"
                role="option"
                aria-selected={o.value === value}
                className={`${styles.option} ${o.value === value ? styles.optionActive : ''}`.trim()}
                onClick={() => {
                  onChange(o.value)
                  setOpen(false)
                }}
              >
                {o.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
