// React HUD sidebar — replaces the SDK7 scene's nav rail. Matches the Explorer 2.0
// design: a 46px rail with a top group (nav/menus) and a bottom group (world tools
// + social). Chat toggles the React chat directly; the other items drive the
// scene's existing menus/popups over the bridge (session.nav) until each is
// migrated to React.

import { useState } from 'react'
import { ControlButton, IconButton, Panel, Toggle } from '../../design'
import type { IconName } from '../../design'
import type { NavAction } from '../../engine/protocol'
import { keyHintFor, useBindingsSnapshot, type BindingsSnapshot } from '../../lib/bindingLabels'
import { nameColor } from '../../lib/identity'
import type { EngineSession } from '../session/useEngineSession'
import { useLiveEventCount } from '../events/eventsApi'
import { useAutoHide } from './useAutoHide'
import styles from './Sidebar.module.css'
import notificationsArt from '../../assets/sidebar-rail/notifications.webp'
import eventsArt from '../../assets/sidebar-rail/events.webp'
import placesArt from '../../assets/sidebar-rail/places.webp'
import communitiesArt from '../../assets/sidebar-rail/communities.webp'
import backpackArt from '../../assets/sidebar-rail/backpack.webp'
import marketplaceArt from '../../assets/sidebar-rail/marketplace.webp'
import galleryArt from '../../assets/sidebar-rail/gallery.webp'
import settingsArt from '../../assets/sidebar-rail/settings.webp'
import helpArt from '../../assets/sidebar-rail/help.webp'
import bugArt from '../../assets/sidebar-rail/bug.webp'
import skyboxArt from '../../assets/sidebar-rail/skybox.webp'
import emotesArt from '../../assets/sidebar-rail/emotes.webp'
import friendsArt from '../../assets/sidebar-rail/friends.webp'
import chatArt from '../../assets/sidebar-rail/chat.webp'
import voiceOffArt from '../../assets/sidebar-rail/voice-off.webp'
import voiceHearingArt from '../../assets/sidebar-rail/voice-hearing.webp'
import voiceSpeakingArt from '../../assets/sidebar-rail/voice-speaking.webp'
import { bugReportUrl } from '../../lib/bugReport'

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
  | { kind: 'places'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'events'; icon: IconName; label: string }
  | { kind: 'skybox'; icon: IconName; label: string }
  | { kind: 'gallery'; icon: IconName; label: string; hotkey?: string }
  | { kind: 'link'; icon: IconName; label: string; url: string | (() => string) }
  | { kind: 'divider' }


// The reference rail's own idle icons, each at the size it draws them.
const RAIL_ART: Partial<Record<IconName, { src: string; size: number }>> = {
  'notifications': { src: notificationsArt, size: 30 },
  'events': { src: eventsArt, size: 32 },
  'places': { src: placesArt, size: 32 },
  'communities': { src: communitiesArt, size: 32 },
  'backpack': { src: backpackArt, size: 28 },
  'marketplace': { src: marketplaceArt, size: 32 },
  'gallery': { src: galleryArt, size: 28 },
  'settings': { src: settingsArt, size: 28 },
  'help': { src: helpArt, size: 32 },
  'bug': { src: bugArt, size: 30 },
  'skybox': { src: skyboxArt, size: 30 },
  'emotes': { src: emotesArt, size: 32 },
  'friends': { src: friendsArt, size: 30 },
  'chat': { src: chatArt, size: 26 },
  'voice-off': { src: voiceOffArt, size: 34 },
  'voice-hearing': { src: voiceHearingArt, size: 34 },
  'voice-speaking': { src: voiceSpeakingArt, size: 34 }
}

function RailButton(props: React.ComponentProps<typeof IconButton>): React.JSX.Element {
  return <IconButton art={RAIL_ART[props.icon]} {...props} />
}

const TOP: Item[] = [
  { kind: 'profile', icon: 'profile', label: 'Profile' },
  { kind: 'notifications', icon: 'notifications', label: 'Notifications' },
  { kind: 'divider' },
  { kind: 'events', icon: 'events', label: 'Events' },
  { kind: 'places', icon: 'places', label: 'Places', hotkey: 'Places' },
  { kind: 'communities', icon: 'communities', label: 'Communities', hotkey: 'Communities' },
  { kind: 'backpack', icon: 'backpack', label: 'Backpack', hotkey: 'Backpack' },
  { kind: 'link', icon: 'marketplace', label: 'Marketplace', url: 'https://decentraland.org/shop?utm_source=client' },
  { kind: 'gallery', icon: 'gallery', label: 'Gallery', hotkey: 'Gallery' },
  { kind: 'settings', icon: 'settings', label: 'Settings', hotkey: 'Settings' },
  { kind: 'divider' },
  { kind: 'link', icon: 'help', label: 'Help & Support', url: 'https://decentraland.org/help/' },
  { kind: 'link', icon: 'bug', label: 'Report a bug', url: bugReportUrl }
]

const BOTTOM: Item[] = [
  { kind: 'mic', icon: 'mic', label: 'Voice chat' },
  { kind: 'skybox', icon: 'skybox', label: 'Skybox' },
  { kind: 'divider' },
  { kind: 'emotes', icon: 'emotes', label: 'Emotes', hotkey: 'Emote' },
  { kind: 'divider' },
  { kind: 'friends', icon: 'friends', label: 'Friends', hotkey: 'Friends' },
  { kind: 'chat', icon: 'chat', label: 'Chat', hotkey: 'ChatPanel' }
]

function renderItem(item: Item, i: number, session: EngineSession, snap: BindingsSnapshot, liveEvents: number, onViewProfile?: () => void): React.JSX.Element {
  if (item.kind === 'divider') return <div key={`d${i}`} className={styles.divider} />
  const shortcut = 'hotkey' in item && item.hotkey != null ? keyHintFor(snap, item.hotkey) : undefined
  if (item.kind === 'chat')
    return (
      <RailButton
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
      <RailButton
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
      <RailButton
        key="settings"
        icon={item.icon}
        label={item.label}
        shortcut={shortcut}
        active={session.settings.open}
        onClick={session.settings.toggle}
      />
    )
  if (item.kind === 'profile') {
    const p = session.profile.data
    return (
      <RailButton
        key="profile"
        icon={item.icon}
        avatar={p ? { src: p.picture, name: p.name, color: nameColor(p.address || p.name) } : undefined}
        label={item.label}
        active={session.profile.open}
        onClick={onViewProfile ?? session.profile.toggle}
      />
    )
  }
  if (item.kind === 'backpack')
    return (
      <RailButton
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
      <RailButton
        key="communities"
        icon={item.icon}
        label={item.label}
        shortcut={shortcut}
        active={session.communities.open}
        onClick={session.communities.toggle}
      />
    )
  if (item.kind === 'places')
    return (
      <RailButton
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
      <RailButton
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
      <RailButton
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
      <RailButton
        key="emotes"
        icon={item.icon}
        label={item.label}
        shortcut={shortcut}
        active={session.emotes.open}
        onClick={session.emotes.toggle}
      />
    )
  if (item.kind === 'mic') {
    const voice = !session.mic.available ? 'off' : session.mic.enabled ? 'speaking' : 'hearing'
    return (
      <RailButton
        key="mic"
        icon={`voice-${voice}`}
        size={34}
        label={item.label}
        data-voice={voice}
        indicator={voice !== 'off'}
        active={session.mic.enabled}
        onClick={session.mic.toggle}
      />
    )
  }
  if (item.kind === 'skybox')
    return <RailButton key="skybox" icon={item.icon} label={item.label} active={session.skybox.open} onClick={session.skybox.toggle} />
  if (item.kind === 'events')
    return (
      <RailButton
        key="events"
        icon={item.icon}
        label={item.label}
        badge={liveEvents}
        badgeTone="lavender"
        active={session.events.open}
        onClick={session.events.toggle}
      />
    )
  if (item.kind === 'link')
    return (
      <RailButton
        key={item.label}
        icon={item.icon}
        label={item.label}
        onClick={() => window.open(typeof item.url === 'string' ? item.url : item.url(), '_blank', 'noopener')}
      />
    )
  return (
    <RailButton
      key={item.action}
      icon={item.icon}
      label={item.label}
      shortcut={shortcut}
      onClick={() => session.nav(item.action)}
    />
  )
}

function DotsGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 8" width="18" height="6" fill="currentColor" aria-hidden="true">
      <circle cx="4" cy="4" r="3" />
      <circle cx="12" cy="4" r="3" />
      <circle cx="20" cy="4" r="3" />
    </svg>
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
  const liveEvents = useLiveEventCount()
  const [configOpen, setConfigOpen] = useState(false)
  const autoHide = useAutoHide(configOpen)
  return (
    <>
      {autoHide.hidden && <div className={styles.reveal} data-testid="sidebar-reveal" onPointerEnter={autoHide.onPointerEnter} />}
      <nav
        className={styles.root}
        aria-label="Main navigation"
        data-hidden={autoHide.hidden}
        onPointerEnter={autoHide.onPointerEnter}
        onPointerLeave={autoHide.onPointerLeave}
      >
        <div className={styles.group}>
          <ControlButton
            size="sm"
            shape="pill"
            variant="solid"
            className={styles.configButton}
            aria-label="Sidebar settings"
            aria-expanded={configOpen}
            active={configOpen}
            onClick={() => setConfigOpen((o) => !o)}
          >
            <DotsGlyph />
          </ControlButton>
          {TOP.map((item, i) => renderItem(item, i, session, snap, liveEvents, onViewProfile))}
        </div>
        <div className={styles.group}>{BOTTOM.map((item, i) => renderItem(item, i, session, snap, liveEvents, onViewProfile))}</div>
      </nav>
      {configOpen && (
        <Panel className={styles.config} role="dialog" aria-label="Sidebar settings">
          <span>Auto-hide sidebar</span>
          <Toggle checked={autoHide.enabled} onChange={autoHide.setEnabled} aria-label="Auto-hide sidebar" />
        </Panel>
      )}
    </>
  )
}
