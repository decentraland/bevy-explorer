// IconButton — square nav/control button used by the HUD sidebar. Idle / hover /
// active states + an optional notification badge, wrapped in the design-system
// Tooltip (label + optional keyboard-shortcut hint, e.g. "Chat [T]").

import { Avatar } from './Avatar'
import { Icon, type IconName } from './icons'
import { Tooltip } from './Tooltip'
import { MaskIcon } from './MaskIcon'
import styles from './IconButton.module.css'

interface IconButtonProps
  extends Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, 'title'> {
  icon: IconName
  /** Selected/active state (e.g. chat open). */
  active?: boolean
  /** Notification count badge (hidden when 0/undefined). */
  badge?: number
  /** Above this the badge reads "+max" (default 99 → "99+"). */
  badgeMax?: number
  /** Badge tone — Ruby (default) or Lavender (e.g. Hangouts). */
  badgeTone?: 'ruby' | 'lavender'
  /** Tooltip label shown on hover. */
  label: string
  /** Single-key shortcut shown dimmed in the tooltip, e.g. 'T' → "Chat [T]". */
  shortcut?: string
  /** Green status dot (e.g. connected voice). */
  indicator?: boolean
  /** Render this profile picture in place of the icon. */
  avatar?: { src?: string; name: string; color?: string }
  /** Art drawn at `size` in place of the icon: white-on-transparent takes the icon colour, `color`
   *  art is drawn as-is. */
  art?: { src: string; size: number; color?: boolean }
  /** 34 for the voice button; 32 otherwise. */
  size?: 32 | 34
}

export function IconButton({
  icon,
  active = false,
  badge,
  badgeMax,
  badgeTone = 'ruby',
  label,
  shortcut,
  avatar,
  art,
  size = 32,
  indicator = false,
  className = '',
  type = 'button',
  children,
  ...rest
}: IconButtonProps): React.JSX.Element {
  return (
    <Tooltip label={label} shortcut={shortcut} side="right" variant="rail">
      <button
        type={type}
        className={`${styles.btn} ${size === 34 ? styles.large : ''} ${active ? styles.active : ''} ${className}`.trim()}
        aria-label={label}
        aria-pressed={active}
        {...rest}
      >
        {avatar ? (
          <Avatar src={avatar.src} name={avatar.name} color={avatar.color} size={30} framed />
        ) : art?.color === true ? (
          <img src={art.src} alt="" width={art.size} height={art.size} draggable={false} />
        ) : art ? (
          <MaskIcon src={art.src} size={art.size} />
        ) : (
          <Icon name={icon} size={24} />
        )}
        {children}
        {indicator && <span className={styles.indicator} data-indicator />}
        {badge != null && badge > 0 && (
          <span
            className={`${styles.badge} ${badgeTone === 'lavender' ? styles.badgeLavender : ''}`.trim()}
          >
            {badgeMax != null ? (badge > badgeMax ? `+${badgeMax}` : badge) : badge > 99 ? '99+' : badge}
          </span>
        )}
      </button>
    </Tooltip>
  )
}
