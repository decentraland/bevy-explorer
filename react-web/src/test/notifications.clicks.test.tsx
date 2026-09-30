import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { NotificationsPanel } from '../features/notifications/NotificationsPanel'
import type { NotificationsState } from '../features/session/useEngineSession'
import { fakeSession } from './harness'

function panel(over: Partial<NotificationsState>): NotificationsState {
  return { ...fakeSession().notifications, open: true, markAllRead: vi.fn(), toggle: vi.fn(), ...over }
}

describe('notifications panel clicks', () => {
  it('Mark all read fires markAllRead (only when unread)', async () => {
    const n = panel({ unread: 2, list: [{ id: 'a', type: 'x', timestamp: '2026', read: false, metadata: {} }] })
    render(<NotificationsPanel notifications={n} />)
    await userEvent.click(screen.getByRole('button', { name: /Mark all read/i }))
    expect(vi.mocked(n.markAllRead)).toHaveBeenCalledTimes(1)
  })

  it('hides Mark all read when nothing is unread', () => {
    render(<NotificationsPanel notifications={panel({ unread: 0 })} />)
    expect(screen.queryByRole('button', { name: /Mark all read/i })).toBeNull()
  })

  it('close button toggles the panel', async () => {
    const n = panel({})
    render(<NotificationsPanel notifications={n} />)
    await userEvent.click(screen.getByRole('button', { name: /Close notifications/i }))
    expect(vi.mocked(n.toggle)).toHaveBeenCalledTimes(1)
  })
})
