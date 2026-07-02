// Dropdown — a string-option select with full keyboard nav (arrows, Home/End,
// Enter, Escape), supporting controlled and uncontrolled use. Ported from
// dcl-react-ui; differs from Select in that options are plain strings.

import { useEffect, useRef, useState } from 'react'
import styles from './Dropdown.module.css'

export interface DropdownProps {
  options: string[]
  value?: string
  defaultValue?: string
  onChange?: (value: string) => void
}

export function Dropdown({ options, value, defaultValue, onChange }: DropdownProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultValue ?? options[0])
  const isControlled = value !== undefined
  const cur = isControlled ? value : internal
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(-1)
  const ref = useRef<HTMLDivElement>(null)
  const btnRef = useRef<HTMLButtonElement>(null)
  const id = useRef('dd' + Math.random().toString(36).slice(2, 8)).current

  useEffect(() => {
    if (!open) return
    const onDoc = (e: MouseEvent): void => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDoc)
    return () => document.removeEventListener('mousedown', onDoc)
  }, [open])

  useEffect(() => {
    if (open) setActive(Math.max(0, options.indexOf(cur ?? '')))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  function pick(opt: string): void {
    if (!isControlled) setInternal(opt)
    onChange?.(opt)
    setOpen(false)
    btnRef.current?.focus()
  }

  function onKey(e: React.KeyboardEvent<HTMLDivElement>): void {
    if (e.key === 'Escape') {
      if (open) {
        e.preventDefault()
        setOpen(false)
      }
      return
    }
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      if (!open) setOpen(true)
      else if (active >= 0) pick(options[active])
      return
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Home' || e.key === 'End') {
      e.preventDefault()
      if (!open) {
        setOpen(true)
        return
      }
      if (e.key === 'Home') setActive(0)
      else if (e.key === 'End') setActive(options.length - 1)
      else if (e.key === 'ArrowDown') setActive((a) => Math.min(options.length - 1, a + 1))
      else setActive((a) => Math.max(0, a - 1))
    }
  }

  return (
    <div className={`${styles.dropdown} ${open ? styles.open : ''}`.trim()} ref={ref} onKeyDown={onKey}>
      <button
        type="button"
        className={styles.btn}
        ref={btnRef}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-activedescendant={open && active >= 0 ? `${id}-${active}` : undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <span className={styles.cur}>{cur}</span>
        <svg viewBox="0 0 12 8" width="11" height="8" aria-hidden="true" className={styles.caret}>
          <path
            d="M1 1.5L6 6.5l5-5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {open && (
        <ul className={styles.menu} role="listbox">
          {options.map((opt, i) => (
            <li
              key={opt}
              id={`${id}-${i}`}
              role="option"
              aria-selected={opt === cur}
              className={`${styles.opt} ${opt === cur ? styles.optActive : ''}`.trim()}
              onClick={() => pick(opt)}
            >
              {opt}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
