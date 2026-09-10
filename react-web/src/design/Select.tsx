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

/** Roughly how tall the list can get (matches `max-height` in the stylesheet). */
const LIST_MAX = 260

/** Should the list open upwards? Only when the room below — inside whatever box would clip it —
 *  cannot hold it and there is more room above. Kept pure so the decision is testable; jsdom has
 *  no layout, so the measuring around it cannot be. */
export function preferUp(space: { above: number; below: number; list: number }): boolean {
  return space.below < space.list && space.above > space.below
}

/** The nearest ancestor that would clip the list (a scrolling panel, a modal card), or the
 *  viewport if nothing does. A dropdown at the bottom of a scroll container is cut off by it long
 *  before it reaches the bottom of the screen. */
function clipBounds(el: HTMLElement): { top: number; bottom: number } {
  for (let node = el.parentElement; node != null; node = node.parentElement) {
    const { overflow, overflowY } = getComputedStyle(node)
    if (`${overflow} ${overflowY}`.split(' ').some((v) => v === 'auto' || v === 'scroll' || v === 'hidden' || v === 'clip')) {
      const r = node.getBoundingClientRect()
      return { top: r.top, bottom: r.bottom }
    }
  }
  return { top: 0, bottom: window.innerHeight }
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
  const [up, setUp] = useState(false)
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
        onClick={() => {
          if (!open && ref.current != null) {
            const r = ref.current.getBoundingClientRect()
            const clip = clipBounds(ref.current)
            setUp(preferUp({ above: r.top - clip.top, below: clip.bottom - r.bottom, list: LIST_MAX }))
          }
          setOpen((o) => !o)
        }}
      >
        <span className={styles.value}>{current?.label ?? value}</span>
        <svg className={`${styles.chev} ${open ? styles.chevOpen : ''}`.trim()} viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2.5 4.5L6 8l3.5-3.5" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <ul className={`${styles.list} ${up ? styles.listUp : ''}`.trim()} role="listbox">
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
