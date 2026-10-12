import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '../features/sidebar/Sidebar'
import { EventsPage } from '../features/events/EventsPage'
import { eventDestination, type DclEvent } from '../features/events/eventsApi'
import { fakeProfileState, fakeSession } from './harness'

const parcel: DclEvent = { id: 'p', name: 'Plaza party', x: -9, y: 12, live: true, start_at: '2026-09-25T10:00:00Z', total_attendees: 4 }
const world: DclEvent = { id: 'w', name: 'Galaga night', x: 0, y: 0, world: true, server: 'crazylagg.dcl.eth', live: true, start_at: '2026-09-25T10:00:00Z' }

// Events responses in call order; hot-scenes / worlds live-data from `presence` (absent = down).
function serveEvents(...responses: (DclEvent[] | 'down')[]): void {
  servePresence({}, ...responses)
}

function servePresence(presence: { hotScenes?: unknown; liveData?: unknown }, ...responses: (DclEvent[] | 'down')[]): void {
  const queue = [...responses]
  const reply = (body: unknown): Response => (body === undefined ? new Response('', { status: 503 }) : new Response(JSON.stringify(body)))
  // 'html' = a 200 gateway page instead of JSON; 'hang' = never answers.
  const presenceReply = (body: unknown): Promise<Response> =>
    body === 'hang' ? new Promise(() => {}) : Promise.resolve(body === 'html' ? new Response('<html>gateway</html>', { headers: { 'content-type': 'text/html' } }) : reply(body))
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string) => {
      if (url.includes('/hot-scenes')) return presenceReply(presence.hotScenes)
      if (url.includes('/live-data')) return presenceReply(presence.liveData)
      const r = queue.shift() ?? []
      return Promise.resolve(r === 'down' ? reply(undefined) : reply({ ok: true, data: r }))
    })
  )
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

  it('counts the people in the scene now on live events, not the RSVPs', async () => {
    servePresence(
      {
        hotScenes: [{ parcels: [[-9, 12], [-9, 13]], usersTotalCount: 11 }],
        liveData: { data: { perWorld: [{ worldName: 'CrazyLagg.dcl.eth', users: 3 }] } }
      },
      [parcel, world]
    )
    render(<EventsPage events={{ open: true, toggle: vi.fn() }} profile={fakeProfileState()} onNavigate={vi.fn()} onTeleport={vi.fn()} onVisitWorld={vi.fn()} />)
    expect(await within(await screen.findByRole('button', { name: 'Plaza party' })).findByText('11')).toBeInTheDocument()
    expect(within(screen.getByRole('button', { name: 'Galaga night' })).getByText('3')).toBeInTheDocument()
  })

  it('shows the events when presence answers with HTML or never answers', async () => {
    servePresence({ hotScenes: 'html', liveData: 'hang' }, [parcel, world])
    render(<EventsPage events={{ open: true, toggle: vi.fn() }} profile={fakeProfileState()} onNavigate={vi.fn()} onTeleport={vi.fn()} onVisitWorld={vi.fn()} />)
    expect(await screen.findByRole('button', { name: 'Plaza party' })).toBeInTheDocument()
    expect(screen.queryByText("Couldn't load events")).not.toBeInTheDocument()
  })

  it('has no destination for a world event without a server', () => {
    expect(eventDestination({ ...world, server: null })).toBeNull()
  })
})
