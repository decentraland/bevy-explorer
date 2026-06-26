// MainMenuShell — the shared full-screen menu chrome (dark top bar + accent line +
// body). Every full-screen menu page (Settings, Backpack, …) renders inside this so
// the top bar is identical and consistent. Pages pass their content as children.

import { DclLogo, Icon, type IconName } from '../../design'
import { ProfileChip } from './ProfileChip'
import styles from './MainMenuShell.module.css'

export interface MenuItem {
  label: string
  icon: IconName
  shortcut?: string
  /** React page id this item opens. */
  page: string
}

// The menu pages we support (others hidden). Matches the Figma nav bar
// (icon + LABEL [shortcut]). Every item is now a React page.
export const MENU_ITEMS: MenuItem[] = [
  { label: 'Communities', icon: 'communities', shortcut: 'O', page: 'communities' },
  { label: 'Map', icon: 'map', shortcut: 'M', page: 'map' },
  { label: 'Backpack', icon: 'backpack', shortcut: 'I', page: 'backpack' },
  { label: 'Settings', icon: 'settings', shortcut: 'P', page: 'settings' }
]

export function MainMenuShell({
  active,
  profileName,
  profilePicture,
  profileAddress,
  profileClaimed,
  onNavigate,
  onClose,
  transparentBody = false,
  children
}: {
  /** page id of the active React page, e.g. 'settings'. */
  active: string
  profileName?: string
  profilePicture?: string
  profileAddress?: string
  profileClaimed?: boolean
  onNavigate: (page: string) => void
  onClose: () => void
  /** Body becomes a pass-through hole for an engine-rendered view (map/avatar). */
  transparentBody?: boolean
  children: React.ReactNode
}): React.JSX.Element {
  return (
    <div className={`${styles.overlay} ${transparentBody ? styles.overlayPass : ''}`.trim()}>
      <header className={styles.topbar}>
        <div className={styles.brand}>
          <DclLogo size={26} />
          <span className={styles.brandName}>Decentraland</span>
        </div>
        <nav className={styles.menu}>
          {MENU_ITEMS.map((m) => (
            <button
              key={m.label}
              type="button"
              className={`${styles.menuItem} ${m.page === active ? styles.menuActive : ''}`.trim()}
              onClick={() => m.page !== active && onNavigate(m.page)}
            >
              <Icon name={m.icon} size={20} />
              <span className={styles.menuLabel}>
                {m.label}
                {m.shortcut && <span className={styles.menuKey}> [{m.shortcut}]</span>}
              </span>
            </button>
          ))}
        </nav>
        {profileName && (
          <ProfileChip
            name={profileName}
            picture={profilePicture}
            address={profileAddress}
            claimed={profileClaimed}
            onViewProfile={() => onNavigate('profile')}
            onSignOut={() => onNavigate('signout')}
            onExit={onClose}
          />
        )}
        <button type="button" className={styles.close} aria-label="Close" onClick={onClose}>
          ×
        </button>
      </header>
      <div className={styles.accent} />
      <div className={`${styles.body} ${transparentBody ? styles.bodyTransparent : ''}`.trim()}>{children}</div>
    </div>
  )
}
