import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { NotificationsPanel } from '../features/notifications/NotificationsPanel'
import type { AppNotification } from '../engine/protocol'
import type { NotificationsState } from '../features/session/useEngineSession'

function panel(list: AppNotification[]): NotificationsState {
  return { list, unread: list.filter((n) => !n.read).length, open: true, toggle: vi.fn(), markAllRead: vi.fn() }
}

describe('NotificationsPanel rendering', () => {
  it('shows the friend name + action (not the raw "Social Service …" type)', () => {
    const list: AppNotification[] = [
      {
        id: 'a',
        type: 'social_service_friendship_request',
        timestamp: new Date().toISOString(),
        read: false,
        metadata: { sender: { name: 'Sharknado', address: '0xabc', profileImageUrl: 'https://x/y.png' } }
      },
      {
        id: 'b',
        type: 'social_service_friendship_accepted',
        timestamp: new Date().toISOString(),
        read: false,
        metadata: { sender: { name: 'Mojito', address: '0xdef', profileImageUrl: '' } }
      }
    ]
    render(<NotificationsPanel notifications={panel(list)} />)

    expect(screen.getByText('Sharknado')).toBeInTheDocument()
    expect(screen.getByText('wants to be your friend!')).toBeInTheDocument()
    expect(screen.getByText('Mojito')).toBeInTheDocument()
    expect(screen.getByText('accepted your friend request.')).toBeInTheDocument()
    expect(screen.queryByText(/Social Service/i)).not.toBeInTheDocument()
  })

  it('uses the server-rendered title/description when present', () => {
    const list: AppNotification[] = [
      { id: 'c', type: 'item_sold', timestamp: new Date().toISOString(), read: true, metadata: { title: 'Item sold', description: 'Sold for 120 MANA.' } }
    ]
    render(<NotificationsPanel notifications={panel(list)} />)
    expect(screen.getByText('Item sold')).toBeInTheDocument()
    expect(screen.getByText('Sold for 120 MANA.')).toBeInTheDocument()
  })

  it('formats title-less types (community_*, credit reminders) instead of the raw humanized type', () => {
    const list: AppNotification[] = [
      { id: 'cp', type: 'community_post_added', timestamp: new Date().toISOString(), read: false, metadata: { communityName: 'Toxic Events', thumbnailUrl: 'https://x/c.png' } },
      { id: 'cr', type: 'credits_reminder_claim_credits', timestamp: new Date().toISOString(), read: false, metadata: {} },
      { id: 'ci', type: 'community_invite_received', timestamp: new Date().toISOString(), read: false, metadata: { communityName: 'SheFi' } }
    ]
    render(<NotificationsPanel notifications={panel(list)} />)
    // No raw humanized type strings.
    expect(screen.queryByText(/Community Post Added/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/Credits Reminder/i)).not.toBeInTheDocument()
    // Real copy instead.
    expect(screen.getByText('New Community Announcement')).toBeInTheDocument()
    expect(screen.getByText('A new announcement was posted in Toxic Events.')).toBeInTheDocument()
    expect(screen.getByText('You have Credits waiting to be claimed.')).toBeInTheDocument()
    expect(screen.getByText("You've been invited to join SheFi.")).toBeInTheDocument()
  })
})
