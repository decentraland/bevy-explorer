// Notifications: the notifications list + persisting "mark as read".
//   from: notifications service via BevyApi.kernelFetch (signed GET list / PUT read).
import { isZone, signed } from '../http'
import type { Ctx } from '../bridge'
import type { AppNotification } from '../../../src/engine/protocol'

const ORG = 'https://notifications.decentraland.org/notifications'
const ZONE = 'https://notifications.decentraland.zone/notifications'

type NotificationRaw = {
  id: string
  type: string
  timestamp: string
  read: boolean
  metadata?: Record<string, unknown>
}

async function base(): Promise<string> {
  return (await isZone()) ? ZONE : ORG
}

export function registerNotifications(ctx: Ctx): void {
  ctx.on('getNotifications', async () => {
    const res = await signed<{ notifications?: NotificationRaw[] }>(`${await base()}?limit=50&from=0`)
    const list = res?.notifications ?? []
    const notifications: AppNotification[] = list.map((n) => ({
      id: n.id,
      type: n.type,
      timestamp: n.timestamp,
      read: n.read,
      metadata: n.metadata ?? {}
    }))
    ctx.send({ kind: 'notifications', notifications })
  })

  ctx.on('markNotificationsRead', (msg) => {
    if (msg.ids.length === 0) return
    void (async () => {
      await signed(`${await base()}/read`, 'PUT', { notificationIds: msg.ids })
    })().catch((e: unknown) => {
      console.error('[notifications] markRead failed', e)
    })
  })
}
