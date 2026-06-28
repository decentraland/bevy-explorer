// React notifications panel — fed by the bridge's getNotifications relay (engine
// fetchNotifications). Notification metadata varies by type, so we derive a generic
// title/body/image/time. Built on the design system. Mark-all-read is local for now
// (relaying read state to the engine is a follow-up).

import { ControlButton } from '../../design'
import type { AppNotification } from '../../engine/protocol'
import type { NotificationsState } from '../session/useEngineSession'
import styles from './NotificationsPanel.module.css'

function humanize(s: string): string {
  return s.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

function str(m: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const k of keys) {
    const v = m[k]
    if (typeof v === 'string' && v) return v
  }
  return undefined
}

/** Read a string field from a nested object (e.g. metadata.sender.name). */
function nested(m: Record<string, unknown>, parent: string, ...keys: string[]): string | undefined {
  const p = m[parent]
  if (p && typeof p === 'object') return str(p as Record<string, unknown>, ...keys)
  return undefined
}

function ago(ts: string): string {
  const t = new Date(ts).getTime()
  if (Number.isNaN(t)) return ''
  const mins = Math.floor((Date.now() - t) / 60000)
  if (mins < 1) return 'now'
  if (mins < 60) return `${mins}m`
  const h = Math.floor(mins / 60)
  if (h < 24) return `${h}h`
  const d = Math.floor(h / 24)
  if (d < 7) return `${d}d`
  return new Date(t).toLocaleDateString([], { month: 'short', day: 'numeric' })
}

function pickImage(m: Record<string, unknown>): string | undefined {
  return str(m, 'image', 'thumbnail', 'thumbnailUrl', 'tokenImage', 'profileImageUrl') ?? nested(m, 'sender', 'profileImageUrl', 'image')
}

// Friendship notifications carry the OTHER user under metadata.sender (name/avatar) and have no
// metadata.title — show the friend's name + a readable action, mirroring unity-explorer.
const FRIENDSHIP_BODY: Record<string, string> = {
  social_service_friendship_request: 'wants to be your friend!',
  social_service_friendship_accepted: 'accepted your friend request.'
}

const cname = (m: Record<string, unknown>): string => str(m, 'communityName') ?? 'a community'

// Notification types whose metadata has NO server-rendered title (community_*, credit reminders, …):
// the generic path would show the raw humanized type ("Community Post Added"), so build the same
// readable copy unity-explorer uses (a header + a line composed from the metadata fields).
const TEMPLATES: Record<string, (m: Record<string, unknown>) => { title: string; body?: string }> = {
  community_post_added: (m) => ({ title: 'New Community Announcement', body: `A new announcement was posted in ${cname(m)}.` }),
  community_invite_received: (m) => ({ title: 'Community Invite Received', body: `You've been invited to join ${cname(m)}.` }),
  community_voice_chat_started: (m) => ({ title: 'Community Voice Stream Started', body: `${cname(m)} is streaming — click to join.` }),
  community_renamed: (m) => ({ title: 'Community Renamed', body: `${str(m, 'oldName') ?? 'A community'} was renamed to ${str(m, 'newName') ?? cname(m)}.` }),
  community_request_to_join_received: (m) => ({ title: 'Membership Request Received', body: `${str(m, 'userName', 'memberName') ?? 'Someone'} wants to join ${cname(m)}.` }),
  community_request_to_join_accepted: (m) => ({ title: 'Membership Request Accepted', body: `You're now a member of ${cname(m)}.` }),
  community_deleted: (m) => ({ title: 'Community Deleted', body: `${cname(m)} has been deleted.` }),
  community_deleted_content_violation: (m) => ({ title: 'Community Deleted', body: `${cname(m)} was deleted for a content violation.` }),
  community_member_banned: (m) => ({ title: 'Banned from Community', body: `You've been banned from ${cname(m)}.` }),
  community_member_removed: (m) => ({ title: 'Removed from Community', body: `You've been removed from ${cname(m)}.` }),
  community_ownership_transferred: (m) => ({ title: 'Community Ownership Transferred', body: `You're now the owner of ${cname(m)}.` }),
  credits_reminder_claim_credits: () => ({ title: 'Marketplace Credits', body: 'You have Credits waiting to be claimed.' }),
  credits_reminder_complete_goals: () => ({ title: 'Marketplace Credits', body: 'Complete your goals to earn Credits.' }),
  credits_reminder_usage: () => ({ title: 'Marketplace Credits', body: "Don't forget to use your Credits." }),
  credits_reminder_usage_24_hours: () => ({ title: 'Marketplace Credits', body: 'Your Credits expire soon — use them now.' }),
  credits_reminder_do_not_miss_out: () => ({ title: 'Marketplace Credits', body: "Don't miss out on your Credits." }),
  credits_new_season_reminder: () => ({ title: 'New Credits Season', body: 'A new Marketplace Credits season has started.' })
}

function summarize(n: AppNotification): { title: string; body?: string; image?: string } {
  const m = n.metadata
  const image = pickImage(m)
  // Friendship: the title is the sender's name (no useful metadata.title).
  if (FRIENDSHIP_BODY[n.type]) {
    return { title: nested(m, 'sender', 'name') ?? str(m, 'title') ?? 'Someone', body: FRIENDSHIP_BODY[n.type], image }
  }
  // Server-rendered copy wins when present (events, rewards, badges, governance, items, referral, …).
  const metaTitle = str(m, 'title')
  if (metaTitle != null) return { title: metaTitle, body: str(m, 'description', 'message'), image }
  // Otherwise the readable client template — never the raw humanized type.
  const tmpl = TEMPLATES[n.type]
  if (tmpl) return { ...tmpl(m), image }
  return { title: humanize(n.type), body: str(m, 'description', 'message', 'name'), image }
}

function NotificationRow({ n }: { n: AppNotification }): React.JSX.Element {
  const { title, body, image } = summarize(n)
  return (
    <div className={`${styles.row} ${n.read ? '' : styles.unread}`.trim()}>
      <div className={styles.thumb}>
        {image ? <img className={styles.img} src={image} alt="" /> : <span className={styles.dot} />}
      </div>
      <div className={styles.info}>
        <span className={styles.title}>{title}</span>
        {body && <span className={styles.bodyText}>{body}</span>}
      </div>
      <span className={styles.time}>{ago(n.timestamp)}</span>
      {!n.read && <span className={styles.unreadDot} aria-label="unread" />}
    </div>
  )
}

export function NotificationsPanel({
  notifications
}: {
  notifications: NotificationsState
}): React.JSX.Element | null {
  if (!notifications.open) return null
  return (
    <div className={styles.root}>
      <header className={styles.head}>
        <span className={styles.heading}>Notifications</span>
        {notifications.unread > 0 && (
          <button type="button" className={styles.markRead} onClick={notifications.markAllRead}>
            Mark all read
          </button>
        )}
        <ControlButton variant="solid" className={styles.closeGlyph} aria-label="Close notifications" onClick={notifications.toggle}>
          ×
        </ControlButton>
      </header>
      <div className={styles.body}>
        {notifications.list.length === 0 ? (
          <div className={styles.empty}>You’re all caught up.</div>
        ) : (
          notifications.list.map((n) => <NotificationRow key={n.id} n={n} />)
        )}
      </div>
    </div>
  )
}
