// Events — full-screen MainMenuShell page (Unity ExplorePanel's Events section): live and upcoming
// events from the public events API; a card jumps to the event's parcel or world.

import { useCallback, useEffect, useState } from 'react'
import { EmptyState, People, Pin, Spinner, Tabs, type TabItem } from '../../design'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { EventsState, ProfileState } from '../session/useEngineSession'
import { eventDestination, eventLocation, fetchEvents, type DclEvent, type EventsList } from './eventsApi'
import cardStyles from '../places/PlaceCard.module.css'
import styles from '../places/PlacesPage.module.css'

const SECTIONS: TabItem<EventsList>[] = [
  { id: 'live', label: 'Live now' },
  { id: 'upcoming', label: 'Upcoming' }
]

function hueOf(id: string): number {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360
  return h
}

function when(e: DclEvent): string {
  if (e.live) return 'Happening now'
  return new Date(e.start_at).toLocaleString(undefined, { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
}

function EventCard({ event, onClick }: { event: DclEvent; onClick: () => void }): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const attendees = event.total_attendees ?? 0
  return (
    <article
      className={cardStyles.card}
      onClick={onClick}
      onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && onClick()}
      role="button"
      tabIndex={0}
      aria-label={event.name}
    >
      <div className={cardStyles.media} style={{ ['--hue' as string]: hueOf(event.id) }}>
        {event.image && !failed && <img className={cardStyles.mediaImg} src={event.image} alt="" draggable={false} onError={() => setFailed(true)} />}
        <div className={cardStyles.badges}>
          <div className={cardStyles.badgeGroup}>
            {event.live && (
              <span className={`${cardStyles.badge} ${cardStyles.live}`}>
                <span className={cardStyles.liveDot} /> LIVE
              </span>
            )}
            {attendees > 0 && (
              <span className={cardStyles.badge}>
                <People size={13} />
                {attendees}
              </span>
            )}
          </div>
        </div>
      </div>
      <div className={cardStyles.body}>
        <div className={cardStyles.info}>
          <span className={cardStyles.title} title={event.name}>{event.name}</span>
          <div className={cardStyles.creatorRow}>
            <span className={cardStyles.by}>{when(event)}</span>
            <span className={`${cardStyles.loc} ${event.world ? cardStyles.locWorld : ''}`.trim()} title={eventLocation(event)}>
              <Pin size={12} />
              {eventLocation(event)}
            </span>
          </div>
        </div>
        <div className={cardStyles.jumpInWrap}>
          <span className={cardStyles.jumpIn}>
            <span>Jump in</span>
          </span>
        </div>
      </div>
    </article>
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
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const retry = useCallback(() => setAttempt((n) => n + 1), [])

  useEffect(() => {
    if (!events.open) return
    const ctrl = new AbortController()
    setLoading(true)
    setError(null)
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
      <div className={styles.toolbar}>
        <Tabs items={SECTIONS} value={section} onChange={setSection} aria-label="Events sections" />
      </div>
      <div className={styles.panel}>
        {loading ? (
          <div className={styles.center}>
            <Spinner size={34} />
          </div>
        ) : error ? (
          <EmptyState variant="inline" tone="error" title="Couldn't load events" subtitle={error} actions={[{ label: 'Retry', onClick: retry }]} />
        ) : list.length === 0 ? (
          <EmptyState
            variant="inline"
            title={section === 'live' ? 'No events live right now' : 'No upcoming events'}
            subtitle={section === 'live' ? 'Check Upcoming to see what is next.' : 'New events show up here when they are scheduled.'}
          />
        ) : (
          <div className={styles.grid}>
            {list.map((e) => (
              <EventCard key={e.id} event={e} onClick={() => visit(e)} />
            ))}
          </div>
        )}
      </div>
    </MainMenuShell>
  )
}
