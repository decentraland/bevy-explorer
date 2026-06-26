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

function summarize(n: AppNotification): { title: string; body?: string; image?: string } {
  const m = n.metadata
  return {
    title: str(m, 'title') ?? humanize(n.type),
    body: str(m, 'description', 'message', 'name'),
    image: str(m, 'image', 'thumbnail', 'profileImageUrl')
  }
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
