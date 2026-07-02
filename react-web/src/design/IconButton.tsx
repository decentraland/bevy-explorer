// IconButton — square nav/control button used by the HUD sidebar. Idle / hover /
// active states + an optional notification badge, wrapped in the design-system
// Tooltip (label + optional keyboard-shortcut hint, e.g. "Chat [T]").

import { Icon, type IconName } from './icons'
import { Tooltip } from './Tooltip'
import styles from './IconButton.module.css'

interface IconButtonProps
  extends Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, 'title'> {
  icon: IconName
  /** Selected/active state (e.g. chat open). */
  active?: boolean
  /** Notification count badge (hidden when 0/undefined). */
  badge?: number
  /** Badge tone — Ruby (default) or Lavender (e.g. Hangouts). */
  badgeTone?: 'ruby' | 'lavender'
  /** Tooltip label shown on hover. */
  label: string
  /** Single-key shortcut shown dimmed in the tooltip, e.g. 'T' → "Chat [T]". */
  shortcut?: string
}

export function IconButton({
  icon,
  active = false,
  badge,
  badgeTone = 'ruby',
  label,
  shortcut,
  className = '',
  type = 'button',
  ...rest
}: IconButtonProps): React.JSX.Element {
  return (
    <Tooltip label={label} shortcut={shortcut} side="right">
      <button
        type={type}
        className={`${styles.btn} ${active ? styles.active : ''} ${className}`.trim()}
        aria-label={label}
        aria-pressed={active}
        {...rest}
      >
        <Icon name={icon} size={24} />
        {badge != null && badge > 0 && (
          <span
            className={`${styles.badge} ${badgeTone === 'lavender' ? styles.badgeLavender : ''}`.trim()}
          >
            {badge > 99 ? '99+' : badge}
          </span>
        )}
      </button>
    </Tooltip>
  )
}
