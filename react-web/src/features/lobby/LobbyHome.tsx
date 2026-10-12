// The lobby: shown after sign-in, before entering the world (and reopened from the menu in-world).
// The stage and the avatar are drawn by the engine behind this transparent page.

import { useCallback, useEffect, useRef, useState } from 'react'
import backdrop from '../../assets/lobby/background.jpg'
import vignette from '../../assets/lobby/vignette.png'
import logo from '../../assets/lobby/logo.png'
import mouseLeft from '../../assets/lobby/mouse-left.png'
import notificationsIcon from '../../assets/lobby/notifications.png'
import { Close, HeaderButton, MaskIcon, Rail } from '../../design'
import { ProfileChip } from '../menu/ProfileChip'
import { BACKDROP_EDGE_FADE, FLOOR, backdropRect, floorShadeGradient } from '../../engine/lobbyStage'
import { userColor } from '../../lib/identity'
import { clip } from '../backpack/BackpackModal'
import { holeClip } from '../../lib/holeClip'
import { EngineViewport } from '../engine/EngineViewport'
import { useStoredProfile } from '../login/useStoredProfile'
import { eventDestination } from '../events/eventsApi'
import { placeCreator, placePlayers, placeTeleport, type DiscoverPlace } from '../places/placesApi'
import { openPassport } from '../profile/Passport'
import { useSession } from '../session/SessionContext'
import type { Destination } from '../session/useEngineSession'
import { FriendCard, LandingCard, LiveEventCard, PlaceCard } from './LobbyCards'
import { eventPeople, fetchHighlighted, fetchHomePlace, fetchLiveEvents, fetchLivePlaces, fetchRecents, type LiveEvent } from './lobbyApi'
import styles from './LobbyHome.module.css'

type Rect = { x: number; y: number; width: number; height: number }

// A failed fetch shows its section empty (hidden), as on parity.
function useLoad<T>(load: () => Promise<T>, fallback: T): { data: T; loading: boolean } {
  const [state, setState] = useState<{ data: T; loading: boolean }>({ data: fallback, loading: true })
  const fallbackRef = useRef(fallback)
  useEffect(() => {
    let live = true
    setState({ data: fallbackRef.current, loading: true })
    load()
      .then((data) => live && setState({ data, loading: false }))
      .catch(() => live && setState({ data: fallbackRef.current, loading: false }))
    return () => {
      live = false
    }
  }, [load])
  return state
}

// The Backpack's preview runs past its modal's edge; only the part inside the modal is a hole.
function clipToModal(rect: Rect | null): Rect | null {
  const frame = document.querySelector('[role="dialog"][aria-label="Backpack"]')?.getBoundingClientRect()
  return rect == null || frame == null ? rect : clip(rect, frame)
}

const FLOOR_COLOR = `rgb(${FLOOR.r}, ${FLOOR.g}, ${FLOOR.b})`
const FLOOR_SHADE = floorShadeGradient()

// Until the engine has drawn the stage, the page paints the same backdrop and the account's
// snapshot, so the lobby never shows an empty or half-loaded centre.
export function StandInStage({ hidden, body }: { hidden: boolean; body?: string }): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ w: 1920, h: 1080 })
  useEffect(() => {
    const el = ref.current
    if (el == null) return
    const ro = new ResizeObserver(() => setSize({ w: el.offsetWidth, h: el.offsetHeight }))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  const rect = backdropRect(size.w, size.h)
  return (
    <div ref={ref} className={`${styles.standIn} ${hidden ? styles.standInHidden : ''}`.trim()} style={{ background: FLOOR_COLOR }} aria-hidden="true">
      <img className={styles.standInBackdrop} src={backdrop} alt="" style={rect} />
      <div
        className={styles.standInEdge}
        style={{ top: rect.top + rect.height - BACKDROP_EDGE_FADE, height: BACKDROP_EDGE_FADE, background: `linear-gradient(to bottom, transparent, ${FLOOR_COLOR})` }}
      />
      <div className={styles.standInShade} style={{ background: FLOOR_SHADE }} />
      <img className={styles.standInVignette} src={vignette} alt="" />
      {body && <img className={styles.standInAvatar} src={body} alt="" draggable={false} />}
    </div>
  )
}

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
  const home = session.homeScene()
  const homeRealm = home?.realm ?? null
  const homeParcel = home?.parcel
  const loadHome = useCallback(
    () => homeParcel == null ? Promise.resolve(null) : fetchHomePlace({ realm: homeRealm, parcel: homeParcel }),
    [homeRealm, homeParcel]
  )
  const landing = useLoad(loadHome, null as DiscoverPlace | null)
  const recents = useLoad(fetchRecents, [] as DiscoverPlace[])
  const recommended = useLoad(fetchHighlighted, [] as DiscoverPlace[])
  const liveEvents = useLoad(fetchLiveEvents, [] as LiveEvent[])
  const livePlaces = useLoad(fetchLivePlaces, [] as DiscoverPlace[])
  const online = session.friends.list
    .filter((f) => f.status !== 'offline')
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
  const rootRef = useRef<HTMLDivElement>(null)
  const [tooltip, setTooltip] = useState<{ x: number; y: number } | null>(null)
  // a pick releases the held world, so none until sign-in has finished
  const waiting = session.login.busy
  const pickPlace = (p: DiscoverPlace): void => onPick(placeTeleport(p))
  // The landing card already offers its place; the rails don't repeat it.
  const notLanding = (list: DiscoverPlace[]): DiscoverPlace[] => list.filter((p) => p.id !== landing.data?.id)
  const recentPlaces = notLanding(recents.data)
  const recommendedPlaces = notLanding(recommended.data)
  const busyPlaces = notLanding(livePlaces.data)
  const landingTitle = landing.data?.title ?? (landing.loading || home == null ? '' : home.realm ?? (home.parcel === '0,0' ? 'Genesis Plaza' : home.parcel))

  // Under the Backpack modal the lobby shows, but with a hole where the engine draws the
  // Backpack's avatar, which is behind it.
  const hole = session.backpack.open ? clipToModal(session.avatarPreviewRect) : null
  const scale = rootRef.current != null ? rootRef.current.getBoundingClientRect().width / rootRef.current.offsetWidth || 1 : 1
  const backpackHole =
    hole == null ? undefined : holeClip({ x: hole.x / scale, y: hole.y / scale, width: hole.width / scale, height: hole.height / scale })

  // The page is scaled to its 1920×1080 canvas, so pointer coordinates are converted into it.
  const trackTooltip = (e: React.MouseEvent): void => {
    const root = rootRef.current
    if (root == null) return
    const box = root.getBoundingClientRect()
    const scale = box.width / root.offsetWidth || 1
    setTooltip({ x: (e.clientX - box.left) / scale, y: (e.clientY - box.top) / scale })
  }

  return (
    <div ref={rootRef} className={styles.root} style={backpackHole}>
      {/* the Backpack modal takes the avatar preview while it's open; the stage comes back after */}
      <div className={styles.stage}>{!session.backpack.open && <EngineViewport region="lobby" report={setEngineViewport} />}</div>
      <StandInStage hidden={session.lobbyStageReady && !session.backpack.open} body={stored.body} />

      <button
        type="button"
        className={styles.avatarHit}
        aria-label="Customize"
        disabled={!session.playerReady}
        onClick={() => {
          setTooltip(null)
          session.backpack.toggle()
        }}
        onMouseMove={trackTooltip}
        onMouseLeave={() => setTooltip(null)}
      />

      <header className={styles.header}>
        <img className={styles.logo} src={logo} alt="Decentraland" />
        <div className={styles.headerRight}>
          <HeaderButton aria-label="Notifications" onClick={session.notifications.toggle}>
            <MaskIcon src={notificationsIcon} size={22} />
          </HeaderButton>
          <ProfileChip
            variant="lobby"
            name={profile?.name ?? ''}
            picture={profile?.picture}
            address={profile?.address}
            claimed={profile?.hasClaimedName}
            onViewProfile={() => profile && openPassport(profile.address)}
            onSignOut={onClose ? undefined : session.logout}
          />
          {onClose && (
            <HeaderButton aria-label="Close" onClick={onClose}>
              <Close size={12} />
            </HeaderButton>
          )}
        </div>
      </header>

      <section className={styles.quickJump}>
        <h1 className={styles.welcome}>{profile?.name ? `Welcome ${profile.name}!` : 'Welcome!'}</h1>
        <LandingCard
          title={landingTitle}
          creator={landing.data ? placeCreator(landing.data) : null}
          image={landing.data?.image ?? null}
          count={landing.data ? placePlayers(landing.data) : null}
          loading={landing.loading || home == null}
          disabled={waiting}
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
              />
            ))}
          </Rail>
        </section>
      )}

      {(liveEvents.data.length > 0 || busyPlaces.length > 0) && (
        <section className={styles.live}>
          <h2 className={`${styles.sectionTitle} ${styles.liveTitle}`}>
            <span className={styles.liveDot} />
            Live Now
          </h2>
          {liveEvents.data.length > 0 && (
            <div className={styles.liveEvents}>
              <Rail perPage={1} gap={8}>
                {liveEvents.data.map((e) => {
                  const dest = eventDestination(e)
                  const go = dest == null || waiting ? null : () => onPick(dest.kind === 'world' ? { kind: 'world', realm: dest.realm } : dest)
                  return <LiveEventCard key={e.id} event={e} people={eventPeople(e)} onJumpIn={go} />
                })}
              </Rail>
            </div>
          )}
          {busyPlaces.length > 0 && (
            <div className={styles.livePlaces}>
              <Rail perPage={1} gap={12}>
                {busyPlaces.map((p) => (
                  <PlaceCard key={p.id} wide title={p.title} creator={placeCreator(p)} image={p.image} count={placePlayers(p)} disabled={waiting} onJumpIn={() => pickPlace(p)} />
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
              <PlaceCard key={p.id} title={p.title} creator={placeCreator(p)} image={p.image} count={placePlayers(p)} disabled={waiting} onJumpIn={() => pickPlace(p)} />
            ))}
          </div>
        </section>
      )}

      {recommendedPlaces.length > 0 && (
        <section className={styles.recommended}>
          <h2 className={styles.sectionTitle}>Recommended Places</h2>
          <Rail perPage={3} gap={8}>
            {recommendedPlaces.map((p) => (
              <PlaceCard key={p.id} title={p.title} creator={placeCreator(p)} image={p.image} count={placePlayers(p)} disabled={waiting} onJumpIn={() => pickPlace(p)} />
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
