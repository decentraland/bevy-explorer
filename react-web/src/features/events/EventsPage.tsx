// Events — full-screen MainMenuShell page (Unity ExplorePanel's Events section): live and upcoming
// events from the public events API; a card jumps to the event's parcel or world.

import { useCallback, useEffect, useState } from 'react'
import { BrowseLoading, BrowsePanel, BrowseToolbar, DiscoverCard, DiscoverGrid, EmptyState, Tabs, type TabItem } from '../../design'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { EventsState, ProfileState } from '../session/useEngineSession'
import { eventDestination, eventLocation, eventPeopleNow, fetchEvents, fetchLiveHeadcounts, type DclEvent, type EventsList, type LiveHeadcounts } from './eventsApi'

const SECTIONS: TabItem<EventsList>[] = [
  { id: 'live', label: 'Live now' },
  { id: 'upcoming', label: 'Upcoming' }
]

function when(e: DclEvent): string {
  if (e.live) return 'Happening now'
  return new Date(e.start_at).toLocaleString(undefined, { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
}

function EventCard({ event, people, onClick }: { event: DclEvent; people: number | undefined; onClick: () => void }): React.JSX.Element {
  return (
    <DiscoverCard
      id={event.id}
      title={event.name}
      image={event.image}
      live={event.live}
      count={people}
      byline={when(event)}
      location={{ text: eventLocation(event), world: event.world === true }}
      label={event.name}
      onClick={onClick}
    />
  )
}

export function EventsPage({
  events,
  profile,
  onNavigate,
  onTeleport,
  onVisitWorld
}: {
  events: EventsState
  profile: ProfileState
  onNavigate: (page: string) => void
  onTeleport: (x: number, y: number) => void
  onVisitWorld: (realm: string) => void
}): React.JSX.Element | null {
  const [section, setSection] = useState<EventsList>('live')
  const [list, setList] = useState<DclEvent[]>([])
  const [headcounts, setHeadcounts] = useState<LiveHeadcounts | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const retry = useCallback(() => setAttempt((n) => n + 1), [])

  useEffect(() => {
    if (!events.open) return
    const ctrl = new AbortController()
    setLoading(true)
    setError(null)
    setHeadcounts(null)
    fetchEvents(section, ctrl.signal).then(
      (data) => {
        setList(data)
        setLoading(false)
      },
      (e: unknown) => {
        if (ctrl.signal.aborted) return
        setError(e instanceof Error ? e.message : 'Could not reach the events service')
        setLoading(false)
      }
    )
    // The list never waits on presence: live counts fill in when (and if) they arrive.
    if (section === 'live') void fetchLiveHeadcounts(ctrl.signal).then((counts) => !ctrl.signal.aborted && setHeadcounts(counts))
    return () => ctrl.abort()
  }, [events.open, section, attempt])

  if (!events.open) return null

  const visit = (e: DclEvent): void => {
    const d = eventDestination(e)
    if (!d) return
    if (d.kind === 'world') onVisitWorld(d.realm)
    else onTeleport(d.x, d.y)
    events.toggle()
  }

  const p = profile.data
  return (
    <MainMenuShell
      active="events"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={events.toggle}
    >
      <BrowseToolbar tabs={<Tabs items={SECTIONS} value={section} onChange={setSection} aria-label="Events sections" />} />
      <BrowsePanel>
        {loading ? (
          <BrowseLoading />
        ) : error ? (
          <EmptyState variant="inline" tone="error" title="Couldn't load events" subtitle={error} actions={[{ label: 'Retry', onClick: retry }]} />
        ) : list.length === 0 ? (
          <EmptyState
            variant="inline"
            title={section === 'live' ? 'No events live right now' : 'No upcoming events'}
            subtitle={section === 'live' ? 'Check Upcoming to see what is next.' : 'New events show up here when they are scheduled.'}
          />
        ) : (
          <DiscoverGrid>
            {list.map((e) => (
              <EventCard key={e.id} event={e} people={section === 'upcoming' ? e.total_attendees : headcounts ? eventPeopleNow(e, headcounts) : undefined} onClick={() => visit(e)} />
            ))}
          </DiscoverGrid>
        )}
      </BrowsePanel>
    </MainMenuShell>
  )
}
