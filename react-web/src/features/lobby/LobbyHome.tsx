// The lobby: shown after sign-in, before entering the world. The engine is up and signed in but
// holds the world back, so the stage and the avatar (drawn by the engine behind this transparent
// page) and the bridge work; picking a destination releases it.

import { useEffect, useRef, useState } from 'react'
import backdrop from '../../assets/lobby/background.jpg'
import logo from '../../assets/lobby/logo.png'
import mouseLeft from '../../assets/lobby/mouse-left.png'
import notificationsIcon from '../../assets/lobby/notifications.png'
import { Avatar, MaskIcon, Rail } from '../../design'
import { userColor } from '../../lib/identity'
import { EngineViewport } from '../engine/EngineViewport'
import { useStoredProfile } from '../login/useStoredProfile'
import { eventDestination } from '../events/eventsApi'
import { placeCreator, placePlayers, placeTeleport, type DiscoverPlace } from '../places/placesApi'
import { openPassport } from '../profile/Passport'
import { useSession } from '../session/SessionContext'
import type { Destination } from '../session/useEngineSession'
import { FriendCard, LandingCard, LiveEventCard, PlaceCard, UpcomingEventCard } from './LobbyCards'
import { fetchHighlighted, fetchLobbyEvents, fetchPlaceAt, fetchRecents, startsIn, type LobbyEvents } from './lobbyApi'
import styles from './LobbyHome.module.css'

type Rect = { x: number; y: number; width: number; height: number }

function useLoad<T>(load: () => Promise<T>, fallback: T): { data: T; loading: boolean } {
  const [state, setState] = useState<{ data: T; loading: boolean }>({ data: fallback, loading: true })
  useEffect(() => {
    let live = true
    load()
      .then((data) => live && setState({ data, loading: false }))
      // the reference treats every lobby fetch error as empty
      .catch(() => live && setState({ data: fallback, loading: false }))
    return () => {
      live = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  return state
}

const GENESIS = { x: 0, y: 0 }
// The backdrop's geometry, shared with the bridge's stage so the stand-in lines up with it.
const BACKDROP_ASPECT = 1595 / 986
const BLEND_HEIGHT = 0.445
const IMAGE_BELOW_BLEND = 0.461

// Until the engine has drawn the stage, the page paints the same backdrop and the account's
// snapshot, so the lobby never shows an empty or half-loaded centre.
function StandInStage({ hidden, body }: { hidden: boolean; body?: string }): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ w: 1920, h: 1080 })
  useEffect(() => {
    const el = ref.current
    if (el == null) return
    const ro = new ResizeObserver(() => setSize({ w: el.offsetWidth, h: el.offsetHeight }))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  const width = Math.max(size.w, size.h * BACKDROP_ASPECT)
  const height = width / BACKDROP_ASPECT
  const top = (1 - BLEND_HEIGHT) * size.h + IMAGE_BELOW_BLEND * height - height
  return (
    <div ref={ref} className={`${styles.standIn} ${hidden ? styles.standInHidden : ''}`.trim()} aria-hidden="true">
      <img className={styles.standInBackdrop} src={backdrop} alt="" style={{ width, height, top, left: (size.w - width) / 2 }} />
      {body && <img className={styles.standInAvatar} src={body} alt="" draggable={false} />}
    </div>
  )
}
const NO_EVENTS: LobbyEvents = { live: [], upcoming: [] }

export function LobbyHome({
  onPick,
  setEngineViewport
}: {
  onPick: (dest: Destination) => void
  setEngineViewport: (region: 'map' | 'avatarPreview' | 'lobby', rect: Rect | null, dpr?: number) => void
}): React.JSX.Element {
  const session = useSession()
  const profile = session.profile.data
  const stored = useStoredProfile(session.login.account ?? undefined)
  const landing = useLoad(() => fetchPlaceAt(GENESIS.x, GENESIS.y), null as DiscoverPlace | null)
  const recents = useLoad(fetchRecents, [] as DiscoverPlace[])
  const recommended = useLoad(fetchHighlighted, [] as DiscoverPlace[])
  const events = useLoad(() => fetchLobbyEvents(), NO_EVENTS)
  const online = session.friends.list
    .filter((f) => f.status !== 'offline')
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
  const [tooltip, setTooltip] = useState<{ x: number; y: number } | null>(null)
  const pickPlace = (p: DiscoverPlace): void => onPick(placeTeleport(p))

  return (
    <div className={styles.root}>
      <div className={styles.stage}>
        <EngineViewport region="lobby" report={setEngineViewport} />
      </div>
      <StandInStage hidden={session.lobbyStageReady} body={stored.body} />

      <div
        className={styles.avatarHit}
        onMouseMove={(e) => setTooltip({ x: e.clientX, y: e.clientY })}
        onMouseLeave={() => setTooltip(null)}
      />

      <header className={styles.header}>
        <img className={styles.logo} src={logo} alt="Decentraland" />
        <div className={styles.headerRight}>
          <button type="button" className={styles.headerButton} aria-label="Notifications">
            <MaskIcon src={notificationsIcon} size={22} />
          </button>
          <div className={styles.profileWidget}>
            <Avatar src={profile?.picture} name={profile?.name ?? ''} size={40} framed />
            <span className={styles.profileName}>{profile?.name ?? ''}</span>
          </div>
        </div>
      </header>

      <section className={styles.quickJump}>
        <h1 className={styles.welcome}>{profile?.name ? `Welcome ${profile.name}!` : 'Welcome!'}</h1>
        <LandingCard
          title={landing.data?.title ?? (landing.loading ? '' : 'Genesis Plaza')}
          creator={landing.data ? placeCreator(landing.data) : null}
          image={landing.data?.image ?? null}
          count={landing.data ? placePlayers(landing.data) : null}
          loading={landing.loading}
          onJumpIn={() => onPick(null)}
        />
      </section>

      {online.length > 0 && (
        <section className={styles.friends}>
          <div className={styles.friendsHead}>
            <h2 className={styles.sectionTitle}>Friends</h2>
            <span className={styles.friendsCount}>{online.length} Online</span>
          </div>
          <Rail perPage={4} gap={8}>
            {online.map((f) => (
              <FriendCard
                key={f.address}
                friend={f}
                color={userColor(f.address, f.name, f.claimed, f.nameColor)}
                where={f.status === 'away' ? 'Away' : 'Online'}
                onOpen={() => openPassport(f.address)}
                onJoin={null}
              />
            ))}
          </Rail>
        </section>
      )}

      {(events.data.live.length > 0 || events.data.upcoming.length > 0) && (
        <section className={styles.events}>
          <h2 className={`${styles.sectionTitle} ${styles.eventsTitle}`}>Events</h2>
          {events.data.live.length > 0 && (
            <div className={styles.liveEvents}>
              <Rail perPage={1} gap={8}>
                {events.data.live.map((e) => {
                  const dest = eventDestination(e)
                  return <LiveEventCard key={e.id} event={e} onJumpIn={() => dest && onPick(dest.kind === 'world' ? { kind: 'world', realm: dest.realm } : dest)} />
                })}
              </Rail>
            </div>
          )}
          {events.data.upcoming.length > 0 && (
            <div className={styles.upcomingEvents}>
              <Rail perPage={1} gap={12}>
                {events.data.upcoming.map((e) => (
                  <UpcomingEventCard key={e.id} event={e} startsIn={startsIn(e.start_at)} />
                ))}
              </Rail>
            </div>
          )}
        </section>
      )}

      {recents.data.length > 0 && (
        <section className={styles.jumpBack}>
          <h2 className={styles.sectionTitle}>Jump Back In</h2>
          <div className={styles.jumpBackRow}>
            {recents.data.map((p) => (
              <PlaceCard key={p.id} title={p.title} creator={placeCreator(p)} image={p.image} count={placePlayers(p)} onJumpIn={() => pickPlace(p)} />
            ))}
          </div>
        </section>
      )}

      {recommended.data.length > 0 && (
        <section className={styles.recommended}>
          <h2 className={styles.sectionTitle}>Recommended Places</h2>
          <Rail perPage={3} gap={8}>
            {recommended.data.map((p) => (
              <PlaceCard key={p.id} title={p.title} creator={placeCreator(p)} image={p.image} count={placePlayers(p)} onJumpIn={() => pickPlace(p)} />
            ))}
          </Rail>
        </section>
      )}

      {tooltip && (
        <div className={styles.tooltip} style={{ left: tooltip.x, top: tooltip.y }}>
          <MaskIcon src={mouseLeft} size={22} />
          Customize
        </div>
      )}
    </div>
  )
}
