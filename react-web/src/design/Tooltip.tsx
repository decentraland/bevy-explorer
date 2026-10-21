// Tooltip — wraps a trigger and shows a label (with an optional keyboard shortcut)
// on hover. Explorer 2.0 style: Shadow surface, 12px radius, Inter 600 14/17 Snow.
// Used by the sidebar IconButton; reusable anywhere a hover hint is needed.

import styles from './Tooltip.module.css'

type Side = 'right' | 'left' | 'top' | 'bottom'

interface TooltipProps {
  label: string
  /** Single-key shortcut shown dimmed, e.g. 'L' → "Friends [L]". */
  shortcut?: string
  side?: Side
  /** rail: the sidebar's tooltip (tighter radius, 34px from the button centre, no fade). */
  variant?: 'default' | 'rail'
  /** A paragraph rather than a name: wraps, keeps line breaks, hangs from the trigger's left edge. */
  multiline?: boolean
  className?: string
  children: React.ReactNode
}

export function Tooltip({
  label,
  shortcut,
  side = 'right',
  variant = 'default',
  multiline = false,
  className = '',
  children
}: TooltipProps): React.JSX.Element {
  const tip = [styles.tip, styles[side], variant === 'rail' && styles.rail, multiline && styles.multiline].filter(Boolean).join(' ')
  return (
    <span className={`${styles.wrap} ${className}`.trim()}>
      {children}
      <span className={tip} role="tooltip">
        {label}
        {shortcut && <span className={styles.shortcut}>[{shortcut}]</span>}
      </span>
    </span>
  )
}
