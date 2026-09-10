// Tabs — the one tab strip for the HUD. Two looks: `pill` (filled pills, the selected one
// accent-orange) and `underline` (bare uppercase labels, the selected one underlined in brand).
// Controlled: `value` + `onChange`. WAI-ARIA tablist/tab with a roving tabindex — Left/Right/
// Home/End move focus AND select, so a strip is keyboard-operable (the panels it heads are not
// `tabpanel`s; the strip only owns selection).

import { useRef } from 'react'
import styles from './Tabs.module.css'

export interface TabItem<T extends string = string> {
  id: T
  label: React.ReactNode
  /** Rendered before the label. */
  icon?: React.ReactNode
  /** Rendered after the label. */
  iconAfter?: React.ReactNode
  /** Count pill after the label; hidden when 0/undefined. */
  badge?: number
  /** Shown but not selectable — e.g. a static heading standing in the strip. */
  disabled?: boolean
}

export interface TabsProps<T extends string> {
  items: readonly TabItem<T>[]
  value: T
  onChange: (id: T) => void
  variant?: 'pill' | 'underline'
  /** Positional overrides only (margins, padding, gap); the look stays the variant's. */
  className?: string
  'aria-label'?: string
}

const KEY_STEP: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1 }

export function Tabs<T extends string>({
  items,
  value,
  onChange,
  variant = 'pill',
  className,
  'aria-label': ariaLabel
}: TabsProps<T>): React.JSX.Element {
  const refs = useRef<(HTMLButtonElement | null)[]>([])

  const select = (i: number): void => {
    const item = items[i]
    if (!item || item.disabled) return
    onChange(item.id)
    refs.current[i]?.focus()
  }

  // Wrap around, skipping disabled items; Home/End go to the first/last enabled one.
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    const enabled = items.map((it, i) => (it.disabled ? -1 : i)).filter((i) => i >= 0)
    if (enabled.length === 0) return
    const cur = items.findIndex((it) => it.id === value)
    let next: number | undefined
    if (e.key in KEY_STEP) {
      const pos = enabled.indexOf(cur)
      next = enabled[(pos + KEY_STEP[e.key] + enabled.length) % enabled.length]
    } else if (e.key === 'Home') next = enabled[0]
    else if (e.key === 'End') next = enabled[enabled.length - 1]
    if (next === undefined) return
    e.preventDefault()
    select(next)
  }

  return (
    <div className={`${styles.tabs} ${styles[variant]} ${className ?? ''}`.trim()} role="tablist" aria-label={ariaLabel} onKeyDown={onKeyDown}>
      {items.map((it, i) => {
        const active = it.id === value
        return (
          <button
            key={it.id}
            ref={(el) => { refs.current[i] = el }}
            type="button"
            role="tab"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            disabled={it.disabled}
            className={`${styles.tab} ${active ? styles.active : ''}`.trim()}
            onClick={() => select(i)}
          >
            {it.icon}
            {it.label}
            {it.iconAfter}
            {it.badge ? <span className={styles.badge}>{it.badge}</span> : null}
          </button>
        )
      })}
    </div>
  )
}
