import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../features/sidebar/Sidebar'
import { EventsPage } from '../features/events/EventsPage'
import { eventDestination, type DclEvent } from '../features/events/eventsApi'
import { fakeProfileState, fakeSession } from './harness'

const parcel: DclEvent = { id: 'p', name: 'Plaza party', x: -9, y: 12, live: true, start_at: '2026-09-25T10:00:00Z', total_attendees: 4 }
const world: DclEvent = { id: 'w', name: 'Galaga night', x: 0, y: 0, world: true, server: 'crazylagg.dcl.eth', live: true, start_at: '2026-09-25T10:00:00Z' }

function serveEvents(...responses: (DclEvent[] | 'down')[]): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn()
  for (const r of responses)
    fetchMock.mockResolvedValueOnce(r === 'down' ? new Response('', { status: 503 }) : new Response(JSON.stringify({ ok: true, data: r })))
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

afterEach(() => vi.unstubAllGlobals())

describe('events', () => {
  it('sidebar shows Events after Notifications with the live count badge', async () => {
    serveEvents([parcel, world])
    const s = fakeSession()
    render(<Sidebar session={s} />)
    const labels = screen.getAllByRole('button').map((b) => b.getAttribute('aria-label'))
    expect(labels[labels.indexOf('Notifications') + 1]).toBe('Events')
    const button = screen.getByRole('button', { name: 'Events' })
    await waitFor(() => expect(within(button).getByText('2')).toBeInTheDocument())
    await userEvent.click(button)
    expect(vi.mocked(s.events.toggle)).toHaveBeenCalledTimes(1)
  })

  it('jumps to a parcel event and to a world event', async () => {
    serveEvents([parcel, world])
    const onTeleport = vi.fn()
    const onVisitWorld = vi.fn()
    const events = { open: true, toggle: vi.fn() }
    render(<EventsPage events={events} profile={fakeProfileState()} onNavigate={vi.fn()} onTeleport={onTeleport} onVisitWorld={onVisitWorld} />)
    await userEvent.click(await screen.findByRole('button', { name: 'Plaza party' }))
    expect(onTeleport).toHaveBeenCalledWith(-9, 12)
    await userEvent.click(screen.getByRole('button', { name: 'Galaga night' }))
    expect(onVisitWorld).toHaveBeenCalledWith('crazylagg.dcl.eth')
    expect(events.toggle).toHaveBeenCalledTimes(2)
  })

  it('shows the failure with a Retry that reloads', async () => {
    serveEvents('down', [parcel])
    render(<EventsPage events={{ open: true, toggle: vi.fn() }} profile={fakeProfileState()} onNavigate={vi.fn()} onTeleport={vi.fn()} onVisitWorld={vi.fn()} />)
    expect(await screen.findByText("Couldn't load events")).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }))
    expect(await screen.findByRole('button', { name: 'Plaza party' })).toBeInTheDocument()
  })

  it('has no destination for a world event without a server', () => {
    expect(eventDestination({ ...world, server: null })).toBeNull()
  })
})
