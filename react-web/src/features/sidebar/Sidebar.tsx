// React HUD sidebar — replaces the SDK7 scene's nav rail. Matches the Explorer 2.0
// design: a 46px rail with a top group (nav/menus) and a bottom group (world tools
// + social). Chat toggles the React chat directly; the other items drive the
// scene's existing menus/popups over the bridge (session.nav) until each is
// migrated to React.

import { IconButton } from '../../design'
import type { IconName } from '../../design'
import type { NavAction } from '../../engine/protocol'
import type { EngineSession } from '../session/useEngineSession'
import styles from './Sidebar.module.css'

type Item =
  | { kind: 'nav'; icon: IconName; label: string; action: NavAction; shortcut?: string }
  | { kind: 'chat'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'friends'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'emotes'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'mic'; icon: IconName; label: string }
  | { kind: 'settings'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'profile'; icon: IconName; label: string }
  | { kind: 'notifications'; icon: IconName; label: string }
  | { kind: 'backpack'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'communities'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'map'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'places'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'gallery'; icon: IconName; label: string; shortcut?: string }
  | { kind: 'help'; icon: IconName; label: string }
  | { kind: 'divider' }

const TOP: Item[] = [
  { kind: 'profile', icon: 'profile', label: 'Profile' },
  { kind: 'notifications', icon: 'notifications', label: 'Notifications' },
  { kind: 'map', icon: 'map', label: 'Map', shortcut: 'M' },
  { kind: 'places', icon: 'places', label: 'Places', shortcut: 'Z' },
  { kind: 'communities', icon: 'communities', label: 'Communities', shortcut: 'O' },
  { kind: 'backpack', icon: 'backpack', label: 'Backpack', shortcut: 'I' },
  { kind: 'gallery', icon: 'gallery', label: 'Gallery', shortcut: 'G' },
  { kind: 'settings', icon: 'settings', label: 'Settings', shortcut: 'P' },
  { kind: 'divider' },
  { kind: 'help', icon: 'help', label: 'Help & Support' }
]

const BOTTOM: Item[] = [
  { kind: 'mic', icon: 'mic', label: 'Voice chat' },
  { kind: 'emotes', icon: 'emotes', label: 'Emotes', shortcut: 'B' },
  { kind: 'divider' },
  { kind: 'friends', icon: 'friends', label: 'Friends', shortcut: 'L' },
  { kind: 'chat', icon: 'chat', label: 'Chat', shortcut: 'T' }
]

function renderItem(item: Item, i: number, session: EngineSession, onViewProfile?: () => void): React.JSX.Element {
  if (item.kind === 'divider') return <div key={`d${i}`} className={styles.divider} />
  if (item.kind === 'chat')
    return (
      <IconButton
        key="chat"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.chat.open}
        onClick={session.chat.toggle}
      />
    )
  if (item.kind === 'friends')
    return (
      <IconButton
        key="friends"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        badge={session.friends.received.length}
        active={session.friends.open}
        onClick={session.friends.toggle}
      />
    )
  if (item.kind === 'settings')
    return (
      <IconButton
        key="settings"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.settings.open}
        onClick={session.settings.toggle}
      />
    )
  if (item.kind === 'profile')
    return (
      <IconButton
        key="profile"
        icon={item.icon}
        label={item.label}
        active={session.profile.open}
        onClick={onViewProfile ?? session.profile.toggle}
      />
    )
  if (item.kind === 'backpack')
    return (
      <IconButton
        key="backpack"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.backpack.open}
        onClick={session.backpack.toggle}
      />
    )
  if (item.kind === 'communities')
    return (
      <IconButton
        key="communities"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.communities.open}
        onClick={session.communities.toggle}
      />
    )
  if (item.kind === 'map')
    return (
      <IconButton
        key="map"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.map.open}
        onClick={session.map.toggle}
      />
    )
  if (item.kind === 'places')
    return (
      <IconButton
        key="places"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.places.open}
        onClick={session.places.toggle}
      />
    )
  if (item.kind === 'gallery')
    return (
      <IconButton
        key="gallery"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.gallery.open}
        onClick={session.gallery.toggle}
      />
    )
  if (item.kind === 'notifications')
    return (
      <IconButton
        key="notifications"
        icon={item.icon}
        label={item.label}
        badge={session.notifications.unread}
        active={session.notifications.open}
        onClick={session.notifications.toggle}
      />
    )
  if (item.kind === 'emotes')
    return (
      <IconButton
        key="emotes"
        icon={item.icon}
        label={item.label}
        shortcut={item.shortcut}
        active={session.emotes.open}
        onClick={session.emotes.toggle}
      />
    )
  if (item.kind === 'mic')
    return (
      <IconButton
        key="mic"
        icon={item.icon}
        label={item.label}
        active={session.mic.enabled}
        onClick={session.mic.toggle}
      />
    )
  if (item.kind === 'help')
    return (
      <IconButton
        key="help"
        icon={item.icon}
        label={item.label}
        onClick={() => window.open('https://decentraland.org/help/', '_blank')}
      />
    )
  return (
    <IconButton
      key={item.action}
      icon={item.icon}
      label={item.label}
      shortcut={item.shortcut}
      onClick={() => session.nav(item.action)}
    />
  )
}

export function Sidebar({
  session,
  onViewProfile
}: {
  session: EngineSession
  /** Open the local player's passport (the profile icon). */
  onViewProfile?: () => void
}): React.JSX.Element {
  return (
    <nav className={styles.root} aria-label="Main navigation">
      <div className={styles.group}>{TOP.map((item, i) => renderItem(item, i, session, onViewProfile))}</div>
      <div className={styles.group}>{BOTTOM.map((item, i) => renderItem(item, i, session, onViewProfile))}</div>
    </nav>
  )
}
