// The lobby: shown after sign-in, before entering the world. The engine is up and signed in but
// holds the world back, so the stage and the avatar (drawn by the engine behind this transparent
// page) and the bridge work; picking a destination releases it.

import { useEffect, useRef, useState } from 'react'
import backdrop from '../../assets/lobby/background.jpg'
import vignette from '../../assets/lobby/vignette.png'
import logo from '../../assets/lobby/logo.png'
import mouseLeft from '../../assets/lobby/mouse-left.png'
import notificationsIcon from '../../assets/lobby/notifications.png'
import { Avatar, Close, MaskIcon, Rail } from '../../design'
import { userColor } from '../../lib/identity'
import { EngineViewport } from '../engine/EngineViewport'
import { useStoredProfile } from '../login/useStoredProfile'
import { eventDestination } from '../events/eventsApi'
import { placeCreator, placePlayers, placeTeleport, type DiscoverPlace } from '../places/placesApi'
import { openPassport } from '../profile/Passport'
import { useSession } from '../session/SessionContext'
import type { Destination } from '../session/useEngineSession'
import { FriendCard, LandingCard, LiveEventCard, PlaceCard } from './LobbyCards'
import { eventPeople, fetchHighlighted, fetchLivePlaces, fetchLobbyEvents, fetchPlaceAt, fetchRecents, type LobbyEvents } from './lobbyApi'
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
// The backdrop's geometry, shared with the bridge's stage so the stand-in lines up with it: the
// stage camera's vertical field of view is fixed, so it scales with the screen height (measured
// against the reference stage at 1920×1200).
const BACKDROP_ASPECT = 1595 / 986
const BACKDROP_HEIGHT = 1.033
const BACKDROP_TOP = -0.17

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
  const height = Math.max(size.h * BACKDROP_HEIGHT, size.w / BACKDROP_ASPECT)
  const width = height * BACKDROP_ASPECT
  const top = size.h * BACKDROP_TOP
  return (
    <div ref={ref} className={`${styles.standIn} ${hidden ? styles.standInHidden : ''}`.trim()} aria-hidden="true">
      <img className={styles.standInBackdrop} src={backdrop} alt="" style={{ width, height, top, left: (size.w - width) / 2 }} />
      <div className={styles.standInShade} />
      <img className={styles.standInVignette} src={vignette} alt="" />
      {body && <img className={styles.standInAvatar} src={body} alt="" draggable={false} />}
    </div>
  )
}
const NO_EVENTS: LobbyEvents = { live: [] }

export function LobbyHome({
  onPick,
  onClose,
  setEngineViewport
}: {
  onPick: (dest: Destination) => void
  /** In-world only: the lobby can be closed back to the world (at startup Jump In is the way out). */
  onClose?: () => void
  setEngineViewport: (region: 'map' | 'avatarPreview' | 'lobby', rect: Rect | null, dpr?: number) => void
}): React.JSX.Element {
  const session = useSession()
  const profile = session.profile.data
  const stored = useStoredProfile(session.login.account ?? undefined)
  const landing = useLoad(() => fetchPlaceAt(GENESIS.x, GENESIS.y), null as DiscoverPlace | null)
  const recents = useLoad(fetchRecents, [] as DiscoverPlace[])
  const recommended = useLoad(fetchHighlighted, [] as DiscoverPlace[])
  const events = useLoad(() => fetchLobbyEvents(), NO_EVENTS)
  const livePlaces = useLoad(fetchLivePlaces, [] as DiscoverPlace[])
  const online = session.friends.list
    .filter((f) => f.status !== 'offline')
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
  const [tooltip, setTooltip] = useState<{ x: number; y: number } | null>(null)
  const pickPlace = (p: DiscoverPlace): void => onPick(placeTeleport(p))
  // The landing card already offers its place; the rails don't repeat it.
  const notLanding = (list: DiscoverPlace[]): DiscoverPlace[] => list.filter((p) => p.id !== landing.data?.id)
  const recentPlaces = notLanding(recents.data)
  const recommendedPlaces = notLanding(recommended.data)
  const busyPlaces = notLanding(livePlaces.data)

  return (
    // hidden under the Backpack: it shows the engine's avatar through a hole the lobby would cover
    <div className={`${styles.root} ${session.backpack.open ? styles.covered : ''}`.trim()}>
      {/* the Backpack takes the avatar preview while it's open; the stage comes back after */}
      <div className={styles.stage}>{!session.backpack.open && <EngineViewport region="lobby" report={setEngineViewport} />}</div>
      <StandInStage hidden={session.lobbyStageReady} body={stored.body} />

      <button
        type="button"
        className={styles.avatarHit}
        aria-label="Customize"
        onClick={() => {
          setTooltip(null)
          session.backpack.toggle()
        }}
        onMouseMove={(e) => setTooltip({ x: e.clientX, y: e.clientY })}
        onMouseLeave={() => setTooltip(null)}
      />

      <header className={styles.header}>
        <img className={styles.logo} src={logo} alt="Decentraland" />
        <div className={styles.headerRight}>
          <button type="button" className={styles.headerButton} aria-label="Notifications" onClick={session.notifications.toggle}>
            <MaskIcon src={notificationsIcon} size={22} />
          </button>
          <button type="button" className={styles.profileWidget} onClick={session.profile.toggle}>
            <Avatar src={profile?.picture} name={profile?.name ?? ''} size={40} framed />
            <span className={styles.profileName}>{profile?.name ?? ''}</span>
          </button>
          {onClose && (
            <button type="button" className={styles.headerButton} aria-label="Close" onClick={onClose}>
              <Close size={12} />
            </button>
          )}
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

      {(events.data.live.length > 0 || busyPlaces.length > 0) && (
        <section className={styles.events}>
          <h2 className={`${styles.sectionTitle} ${styles.eventsTitle}`}>
            <span className={styles.liveDot} />
            Live Now
          </h2>
          {events.data.live.length > 0 && (
            <div className={styles.liveEvents}>
              <Rail perPage={1} gap={8}>
                {events.data.live.map((e) => {
                  const dest = eventDestination(e)
                  return <LiveEventCard key={e.id} event={e} people={eventPeople(e)} onJumpIn={() => dest && onPick(dest.kind === 'world' ? { kind: 'world', realm: dest.realm } : dest)} />
                })}
              </Rail>
            </div>
          )}
          {busyPlaces.length > 0 && (
            <div className={styles.livePlaces}>
              <Rail perPage={1} gap={12}>
                {busyPlaces.map((p) => (
                  <PlaceCard key={p.id} wide title={p.title} creator={placeCreator(p)} image={p.image} count={placePlayers(p)} onJumpIn={() => pickPlace(p)} />
                ))}
              </Rail>
            </div>
          )}
        </section>
      )}

      {recentPlaces.length > 0 && (
        <section className={styles.jumpBack}>
          <h2 className={styles.sectionTitle}>Jump Back In</h2>
          <div className={styles.jumpBackRow}>
            {recentPlaces.map((p) => (
              <PlaceCard key={p.id} title={p.title} creator={placeCreator(p)} image={p.image} count={placePlayers(p)} onJumpIn={() => pickPlace(p)} />
            ))}
          </div>
        </section>
      )}

      {recommendedPlaces.length > 0 && (
        <section className={styles.recommended}>
          <h2 className={styles.sectionTitle}>Recommended Places</h2>
          <Rail perPage={3} gap={8}>
            {recommendedPlaces.map((p) => (
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
