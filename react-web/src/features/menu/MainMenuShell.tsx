// MainMenuShell — the shared full-screen menu chrome (dark top bar + accent line +
// body). Every full-screen menu page (Settings, Backpack, …) renders inside this so
// the top bar is identical and consistent. Pages pass their content as children.

import { useEffect, useState } from 'react'
import { DclLogo, Icon, type IconName } from '../../design'
import { keyHintFor, useBindingsSnapshot } from '../../lib/bindingLabels'
import { ProfileChip } from './ProfileChip'
import styles from './MainMenuShell.module.css'

// How many menu shells are currently mounted. Each full-screen page renders its own shell, so
// SWITCHING pages unmounts one and mounts another. A shell that mounts while another is already open
// is a page switch (skip the entrance fade so the shared chrome doesn't flash "close → reopen"); a
// shell that mounts with none open is a fresh open (animate). The previous shell's unmount cleanup
// runs AFTER the new shell renders, so reading this at render time correctly sees the outgoing shell.
let openShells = 0

export interface MenuItem {
  label: string
  icon: IconName
  /** Engine SystemAction whose live binding renders as the [K] hint. */
  hotkey?: string
  /** React page id this item opens. */
  page: string
}

// The menu pages we support (others hidden). Matches the Figma nav bar
// (icon + LABEL [shortcut]). Every item is now a React page.
export const MENU_ITEMS: MenuItem[] = [
  { label: 'Communities', icon: 'communities', hotkey: 'Communities', page: 'communities' },
  { label: 'Places', icon: 'places', hotkey: 'Places', page: 'places' },
  { label: 'Map', icon: 'map', hotkey: 'Map', page: 'map' },
  { label: 'Backpack', icon: 'backpack', hotkey: 'Backpack', page: 'backpack' },
  { label: 'Gallery', icon: 'gallery', hotkey: 'Gallery', page: 'gallery' },
  { label: 'Settings', icon: 'settings', hotkey: 'Settings', page: 'settings' }
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
  // Animate the entrance only on a fresh open (no other shell mounted), not on page switches.
  const [animate] = useState(() => openShells === 0)
  const bindingsSnap = useBindingsSnapshot()
  useEffect(() => {
    openShells++
    return () => {
      openShells--
    }
  }, [])

  return (
    <div className={`${styles.overlay} ${animate ? styles.animateIn : ''} ${transparentBody ? styles.overlayPass : ''}`.trim()}>
      <header className={styles.topbar}>
        <div className={styles.brand}>
          <DclLogo size={26} />
          <span className={styles.brandName}>Decentraland</span>
        </div>
        <nav className={styles.menu}>
          {MENU_ITEMS.map((m) => {
            const shortcut = m.hotkey != null ? keyHintFor(bindingsSnap, m.hotkey) : undefined
            return (
              <button
                key={m.label}
                type="button"
                className={`${styles.menuItem} ${m.page === active ? styles.menuActive : ''}`.trim()}
                onClick={() => m.page !== active && onNavigate(m.page)}
              >
                <Icon name={m.icon} size={20} />
                <span className={styles.menuLabel}>
                  {m.label}
                  {shortcut && <span className={styles.menuKey}> [{shortcut}]</span>}
                </span>
              </button>
            )
          })}
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
