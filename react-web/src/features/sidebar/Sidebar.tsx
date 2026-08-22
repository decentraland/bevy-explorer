// React HUD sidebar — replaces the SDK7 scene's nav rail. Matches the Explorer 2.0
// design: a 46px rail with a top group (nav/menus) and a bottom group (world tools
// + social). Chat toggles the React chat directly; the other items drive the
// scene's existing menus/popups over the bridge (session.nav) until each is
// migrated to React.

import { IconButton } from '../../design'
import type { IconName } from '../../design'
import type { NavAction } from '../../engine/protocol'
import { keyHintFor, useBindingsSnapshot, type BindingsSnapshot } from '../../lib/bindingLabels'
import type { EngineSession } from '../session/useEngineSession'
import styles from './Sidebar.module.css'

// `hotkey` names the engine SystemAction whose live binding renders as the tooltip hint.
type Item =
  | { kind: 'nav'; icon: IconName; label: string; action: NavAction; hotkey?: string }
  | { kind: 'chat'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'friends'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'emotes'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'mic'; icon: IconName; label: string }
  | { kind: 'settings'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'profile'; icon: IconName; label: string }
  | { kind: 'notifications'; icon: IconName; label: string }
  | { kind: 'backpack'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'communities'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'map'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'places'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'gallery'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'help'; icon: IconName; label: string }
  | { kind: 'divider' }

const TOP: Item[] = [
  { kind: 'profile', icon: 'profile', label: 'Profile' },
  { kind: 'notifications', icon: 'notifications', label: 'Notifications' },
  { kind: 'map', icon: 'map', label: 'Map', hotkey: 'Map' },
  { kind: 'places', icon: 'places', label: 'Places', hotkey: 'Places' },
  { kind: 'communities', icon: 'communities', label: 'Communities', hotkey: 'Communities' },
  { kind: 'backpack', icon: 'backpack', label: 'Backpack', hotkey: 'Backpack' },
  { kind: 'gallery', icon: 'gallery', label: 'Gallery', hotkey: 'Gallery' },
  { kind: 'settings', icon: 'settings', label: 'Settings', hotkey: 'Settings' },
  { kind: 'divider' },
  { kind: 'help', icon: 'help', label: 'Help & Support' }
]

const BOTTOM: Item[] = [
  { kind: 'mic', icon: 'mic', label: 'Voice chat' },
  { kind: 'emotes', icon: 'emotes', label: 'Emotes', hotkey: 'Emote' },
  { kind: 'divider' },
  { kind: 'friends', icon: 'friends', label: 'Friends', hotkey: 'Friends' },
  { kind: 'chat', icon: 'chat', label: 'Chat', hotkey: 'ChatPanel' }
]

function renderItem(item: Item, i: number, session: EngineSession, snap: BindingsSnapshot, onViewProfile?: () => void): React.JSX.Element {
  if (item.kind === 'divider') return <div key={`d${i}`} className={styles.divider} />
  const shortcut = 'hotkey' in item && item.hotkey != null ? keyHintFor(snap, item.hotkey) : undefined
  if (item.kind === 'chat')
    return (
      <IconButton
        key="chat"
        icon={item.icon}
        label={item.label}
        shortcut={shortcut}
        badge={session.chat.unread}
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
        shortcut={shortcut}
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
        shortcut={shortcut}
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
        shortcut={shortcut}
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
        shortcut={shortcut}
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
        shortcut={shortcut}
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
        shortcut={shortcut}
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
        shortcut={shortcut}
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
        shortcut={shortcut}
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
        onClick={() => window.open('https://decentraland.org/help/', '_blank', 'noopener')}
      />
    )
  return (
    <IconButton
      key={item.action}
      icon={item.icon}
      label={item.label}
      shortcut={shortcut}
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
  const snap = useBindingsSnapshot()
  return (
    <nav className={styles.root} aria-label="Main navigation">
      <div className={styles.group}>{TOP.map((item, i) => renderItem(item, i, session, snap, onViewProfile))}</div>
      <div className={styles.group}>{BOTTOM.map((item, i) => renderItem(item, i, session, snap, onViewProfile))}</div>
    </nav>
  )
}
