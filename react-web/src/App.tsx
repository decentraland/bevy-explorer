import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { BridgeClient } from './engine/bridge'
import { EngineDriver } from './engine/EngineDriver'
import { EngineRpc } from './engine/engineRpc'
import { EngineHost } from './features/engine/EngineHost'
import type { LoginDriver } from './engine/driver'
import { startMockBridge } from './engine/mockBridge'
// Dev-only (?showcase=1) design gallery — lazy so it never ships in the prod HUD path.
const Showcase = lazy(() => import('./design/Showcase').then((m) => ({ default: m.Showcase })))
import { Chat } from './features/chat/Chat'
import { FriendsPanel } from './features/friends/FriendsPanel'
import { SettingsPanel } from './features/settings/SettingsPanel'
import { ProfilePanel } from './features/profile/ProfilePanel'
import { NotificationsPanel } from './features/notifications/NotificationsPanel'
import { EmotesWheel } from './features/emotes/EmotesWheel'
import { BackpackPage } from './features/backpack/BackpackPage'
import { CommunitiesPage } from './features/communities/CommunitiesPage'
import { MapPage } from './features/map/MapPage'
import { PlacesPage } from './features/places/PlacesPage'
import { PlacesPicker } from './features/places/PlacesPicker'
import { GalleryPage } from './features/gallery/GalleryPage'
import { Sidebar } from './features/sidebar/Sidebar'
import { Pointer } from './features/pointer/Pointer'
import { ProfilePassport } from './features/profile/ProfilePassport'
import { WorldVisitModal } from './components/WorldVisitModal'
import type { ChatUser } from './features/chat/ProfileCard'
import type { Profile } from './engine/protocol'
import { FpsMeter } from './features/debug/FpsMeter'
import { LoadingAndLogin } from './features/login/LoadingAndLogin'
import { SceneLoadingOverlay } from './features/session/SceneLoadingOverlay'
import { useEngineSession } from './features/session/useEngineSession'
import { useHudScale } from './lib/useHudScale'
import { useGlobalHotkey } from './lib/useGlobalHotkey'
import { useMenuShortcuts } from './lib/useMenuShortcuts'

const params = new URLSearchParams(location.search)
// MOCK (?mock=1): UI only, no engine, fake bridge (?previousLogin=1 → returning user).
// ENGINE (default): real engine in a same-origin iframe + super-user bridge scene.
const MODE: 'mock' | 'engine' = params.get('mock') === '1' ? 'mock' : 'engine'
const SHOWCASE = params.get('showcase') === '1'

export function App(): React.JSX.Element {
  useHudScale() // keep --ui-scale in sync with the viewport (DPI-correct, like Unity)
  const showFps = useFpsToggle()
  return (
    <>
      {SHOWCASE ? (
        <Suspense fallback={null}>
          <Showcase />
        </Suspense>
      ) : (
        <Hud />
      )}
      {showFps && <FpsMeter />}
    </>
  )
}

// Perf overlay visibility: on via ?fps=1, toggle anytime with Ctrl/Cmd+Shift+F
// (works even when the engine iframe holds keyboard focus — see useGlobalHotkey).
function useFpsToggle(): boolean {
  const [on, setOn] = useState(params.get('fps') === '1')
  useGlobalHotkey(
    (e) => (e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'F' || e.key === 'f'),
    () => setOn((v) => !v)
  )
  return on
}

function Hud(): React.JSX.Element {
  const { rpc, createDriver } = useMemo((): {
    rpc: EngineRpc | null
    createDriver: () => LoginDriver
  } => {
    if (MODE === 'mock') {
      startMockBridge()
      return { rpc: null, createDriver: () => new BridgeClient() }
    }
    const engineRpc = new EngineRpc()
    return { rpc: engineRpc, createDriver: () => new EngineDriver(engineRpc) }
  }, [])

  const session = useEngineSession(createDriver)
  useMenuShortcuts(session) // [O]/[M]/[I]/[G]/[P]/[B]/[L]/[T] hints in the nav + sidebar

  // Passport (View Profile). Self → the local rich profile; others → the fetched
  // passport (requestUserProfile on open), falling back to identity-only while it loads.
  const [passport, setPassport] = useState<ChatUser | null>(null)
  // A world (e.g. boedo.dcl.eth) the user asked to jump to — drives the shared confirm modal.
  const [visitWorld, setVisitWorld] = useState<string | null>(null)
  // Which tab the Backpack opens on. The emote wheel's "Customise [E]" opens it on Emotes; it resets
  // to Wearables once the Backpack closes so a normal (sidebar/topbar) open lands on Wearables.
  const [backpackTab, setBackpackTab] = useState<'wearables' | 'emotes'>('wearables')
  useEffect(() => {
    if (!session.backpack.open) setBackpackTab('wearables')
  }, [session.backpack.open])
  const isSelfPassport =
    !!passport && !!session.profile.data && session.profile.data.address.toLowerCase() === passport.address.toLowerCase()
  const openPassport = (user: ChatUser): void => {
    setPassport(user)
    if (user.address) session.requestUserProfile(user.address) // fetch badges/photos/catalyst data
  }
  // Friendship status for a user — drives the profile menu's CTA (chat + friends list).
  const relationshipOf = (address: string): 'none' | 'requested' | 'friend' => {
    const a = address.toLowerCase()
    if (session.friends.list.some((f) => f.address.toLowerCase() === a)) return 'friend'
    if (session.friends.sent.some((r) => r.address.toLowerCase() === a)) return 'requested'
    return 'none'
  }
  // Open MY OWN passport (Sidebar profile icon + the menu's "View Profile").
  const viewMyProfile = (): void => {
    const me = session.profile.data
    if (me) openPassport({ address: me.address, name: me.name, picture: me.picture })
  }
  const passportProfile: Profile | null = !passport
    ? null
    : // Prefer the fetched passport (badges/photos/about), even for self; fall back to the
      // local profile (self) or identity-only (others) while the fetch is in flight.
      session.userProfiles[passport.address.toLowerCase()] ??
      (isSelfPassport
        ? session.profile.data
        : {
            address: passport.address,
            name: passport.name,
            picture: passport.picture,
            hasClaimedName: !passport.name.includes('#') && !/^0x[0-9a-f]+$/i.test(passport.name),
            isGuest: false
          })

  // Top-nav navigation between the full-screen menu pages (Settings/Backpack/Map)
  // and the Communities panel. Each toggle is mutually exclusive.
  const goToMenuPage = (page: string): void => {
    if (page === 'settings') session.settings.toggle()
    else if (page === 'backpack') session.backpack.toggle()
    else if (page === 'communities') session.communities.toggle()
    else if (page === 'map') session.map.toggle()
    else if (page === 'places') session.places.toggle()
    else if (page === 'gallery') session.gallery.toggle()
    // Profile-chip actions (forwarded from MainMenuShell's ProfileChip): View Profile
    // opens the full passport (same as for other users), not the small profile card.
    else if (page === 'profile') viewMyProfile()
    else if (page === 'signout') session.logout()
  }

  // A full-screen MainMenuShell page is open (covers the whole HUD).
  const pageOpen =
    session.settings.open || session.backpack.open || session.communities.open || session.map.open || session.places.open || session.gallery.open

  return (
    <>
      {rpc && <EngineHost rpc={rpc} />}
      {session.phase === 'login' && <LoadingAndLogin flow={session.login} />}
      {session.phase === 'picking' && <PlacesPicker onPick={session.pickDestination} />}
      {session.phase === 'entering' && <SceneLoadingOverlay scene={session.scene} />}
      {session.phase === 'world' && !session.menuOpen && (
        <>
          {/* The full-screen menu pages own the whole screen; hide the rail + chat so
              they don't show through (the map page's body is transparent). */}
          {!pageOpen && <Sidebar session={session} onViewProfile={viewMyProfile} />}
          {/* Reticle (when pointer-locked) + world-hover prompt — hidden under a full-screen page. */}
          {!pageOpen && <Pointer hover={session.hover} locked={session.cursorLocked} proximity={session.proximity} />}
          <Chat
            chat={session.chat}
            hidden={session.friends.open || pageOpen}
            me={session.profile.data}
            onAddFriend={(address) => session.friends.act('request', address)}
            onBlock={(address) => session.friends.act('block', address)}
            onViewProfile={openPassport}
            onTeleport={(x, y) => session.map.teleport(x, y)}
            onVisitWorld={(name) => setVisitWorld(name)}
            relationshipOf={relationshipOf}
          />
          <FriendsPanel
            friends={session.friends}
            me={session.profile.data}
            relationshipOf={relationshipOf}
            onAddFriend={(address) => session.friends.act('request', address)}
            onViewProfile={openPassport}
            onBlock={(address) => session.friends.act('block', address)}
          />
          <SettingsPanel settings={session.settings} profile={session.profile} onNavigate={goToMenuPage} />
          <ProfilePanel profile={session.profile} />
          <NotificationsPanel notifications={session.notifications} />
          <EmotesWheel
            emotes={session.emotes}
            onCustomise={() => {
              setBackpackTab('emotes')
              session.backpack.toggle() // exclusive → closes the wheel, opens the backpack
            }}
          />
          <BackpackPage backpack={session.backpack} emotes={session.emotes} profile={session.profile} onNavigate={goToMenuPage} setEngineViewport={session.setEngineViewport} initialTab={backpackTab} />
          <CommunitiesPage
            communities={session.communities}
            profile={session.profile}
            onNavigate={goToMenuPage}
            onAddFriend={(address) => session.friends.act('request', address)}
            onOpenChat={() => session.chat.toggle()}
          />
          <MapPage map={session.map} profile={session.profile} onNavigate={goToMenuPage} />
          <PlacesPage
            places={session.places}
            profile={session.profile}
            onNavigate={goToMenuPage}
            onTeleport={(x, y) => session.map.teleport(x, y)}
            onVisitWorld={(realm) => session.map.changeRealm(realm)}
          />
          <GalleryPage
            gallery={session.gallery}
            profile={session.profile}
            onNavigate={goToMenuPage}
            onTeleport={(x, y) => session.map.teleport(x, y)}
            onViewProfile={openPassport}
          />
          {passport && passportProfile && (
            <ProfilePassport
              key={passport.address}
              profile={passportProfile}
              isSelf={isSelfPassport}
              isFriend={session.friends.list.some((f) => f.address.toLowerCase() === passport.address.toLowerCase())}
              requested={session.friends.sent.some((r) => r.address.toLowerCase() === passport.address.toLowerCase())}
              onAddFriend={(address) => session.friends.act('request', address)}
              onClose={() => setPassport(null)}
            />
          )}
          {visitWorld && (
            <WorldVisitModal
              worldName={visitWorld}
              onCancel={() => setVisitWorld(null)}
              onConfirm={() => {
                session.map.changeRealm(visitWorld)
                setVisitWorld(null)
              }}
            />
          )}
        </>
      )}
    </>
  )
}
