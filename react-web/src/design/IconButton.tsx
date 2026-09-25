// IconButton — square nav/control button used by the HUD sidebar. Idle / hover /
// active states + an optional notification badge, wrapped in the design-system
// Tooltip (label + optional keyboard-shortcut hint, e.g. "Chat [T]").

import { Avatar } from './Avatar'
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
  /** Render this profile picture in place of the icon. */
  avatar?: { src?: string; name: string; color?: string }
}

export function IconButton({
  icon,
  active = false,
  badge,
  badgeTone = 'ruby',
  label,
  shortcut,
  avatar,
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
        {avatar ? <Avatar src={avatar.src} name={avatar.name} color={avatar.color} size={24} /> : <Icon name={icon} size={24} />}
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
