import { useMemo, useState } from 'react'
import { BridgeClient } from './engine/bridge'
import { EngineDriver } from './engine/EngineDriver'
import { EngineRpc } from './engine/engineRpc'
import { EngineHost } from './features/engine/EngineHost'
import type { LoginDriver } from './engine/driver'
import { startMockBridge } from './engine/mockBridge'
import { Showcase } from './design/Showcase'
import { Chat } from './features/chat/Chat'
import { FriendsPanel } from './features/friends/FriendsPanel'
import { SettingsPanel } from './features/settings/SettingsPanel'
import { ProfilePanel } from './features/profile/ProfilePanel'
import { NotificationsPanel } from './features/notifications/NotificationsPanel'
import { EmotesWheel } from './features/emotes/EmotesWheel'
import { BackpackPage } from './features/backpack/BackpackPage'
import { CommunitiesPage } from './features/communities/CommunitiesPage'
import { MapPage } from './features/map/MapPage'
import { Sidebar } from './features/sidebar/Sidebar'
import { Pointer } from './features/pointer/Pointer'
import { ProfilePassport } from './features/profile/ProfilePassport'
import type { ChatUser } from './features/chat/ProfileCard'
import type { Profile } from './engine/protocol'
import { LoadingAndLogin } from './features/login/LoadingAndLogin'
import { SceneLoadingOverlay } from './features/session/SceneLoadingOverlay'
import { useEngineSession } from './features/session/useEngineSession'
import { useHudScale } from './lib/useHudScale'

const params = new URLSearchParams(location.search)
// MOCK (?mock=1): UI only, no engine, fake bridge (?previousLogin=1 → returning user).
// ENGINE (default): real engine in a same-origin iframe + super-user bridge scene.
const MODE: 'mock' | 'engine' = params.get('mock') === '1' ? 'mock' : 'engine'
const SHOWCASE = params.get('showcase') === '1'

export function App(): React.JSX.Element {
  useHudScale() // keep --ui-scale in sync with the viewport (DPI-correct, like Unity)
  if (SHOWCASE) return <Showcase />
  return <Hud />
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

  // Passport (View Profile). Self → the local rich profile; others → the fetched
  // passport (requestUserProfile on open), falling back to identity-only while it loads.
  const [passport, setPassport] = useState<ChatUser | null>(null)
  const isSelfPassport =
    !!passport && !!session.profile.data && session.profile.data.address.toLowerCase() === passport.address.toLowerCase()
  const openPassport = (user: ChatUser): void => {
    setPassport(user)
    if (user.address) session.requestUserProfile(user.address)
  }
  const passportProfile: Profile | null = !passport
    ? null
    : isSelfPassport
      ? session.profile.data
      : session.userProfiles[passport.address.toLowerCase()] ?? {
          // Shown immediately (identity) while the fetch is in flight / if it has no profile.
          address: passport.address,
          name: passport.name,
          picture: passport.picture,
          hasClaimedName: !passport.name.includes('#') && !/^0x[0-9a-f]+$/i.test(passport.name),
          isGuest: false
        }

  // Top-nav navigation between the full-screen menu pages (Settings/Backpack/Map)
  // and the Communities panel. Each toggle is mutually exclusive.
  const goToMenuPage = (page: string): void => {
    if (page === 'settings') session.settings.toggle()
    else if (page === 'backpack') session.backpack.toggle()
    else if (page === 'communities') session.communities.toggle()
    else if (page === 'map') session.map.toggle()
    // Profile-chip actions (forwarded from MainMenuShell's ProfileChip).
    else if (page === 'profile') session.profile.toggle()
    else if (page === 'signout') session.logout()
  }

  // A full-screen MainMenuShell page is open (covers the whole HUD).
  const pageOpen =
    session.settings.open || session.backpack.open || session.communities.open || session.map.open

  return (
    <>
      {rpc && <EngineHost rpc={rpc} />}
      {session.phase === 'login' && <LoadingAndLogin flow={session.login} />}
      {session.phase === 'entering' && <SceneLoadingOverlay scene={session.scene} />}
      {session.phase === 'world' && !session.menuOpen && (
        <>
          {/* The full-screen menu pages own the whole screen; hide the rail + chat so
              they don't show through (the map page's body is transparent). */}
          {!pageOpen && <Sidebar session={session} />}
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
          />
          <FriendsPanel friends={session.friends} />
          <SettingsPanel settings={session.settings} profile={session.profile} onNavigate={goToMenuPage} />
          <ProfilePanel profile={session.profile} />
          <NotificationsPanel notifications={session.notifications} />
          <EmotesWheel emotes={session.emotes} />
          <BackpackPage backpack={session.backpack} emotes={session.emotes} profile={session.profile} onNavigate={goToMenuPage} setEngineViewport={session.setEngineViewport} />
          <CommunitiesPage
            communities={session.communities}
            profile={session.profile}
            onNavigate={goToMenuPage}
            onAddFriend={(address) => session.friends.act('request', address)}
            onOpenChat={() => session.chat.toggle()}
          />
          <MapPage map={session.map} profile={session.profile} onNavigate={goToMenuPage} />
          {passport && passportProfile && (
            <ProfilePassport
              profile={passportProfile}
              isFriend={session.friends.list.some((f) => f.address.toLowerCase() === passport.address.toLowerCase())}
              // The engine cutout renders the LOCAL avatar, so only use it for self;
              // other users show their full-body snapshot from the fetched passport.
              useEngineViewport={isSelfPassport}
              onAddFriend={(address) => session.friends.act('request', address)}
              onClose={() => setPassport(null)}
              setEngineViewport={session.setEngineViewport}
            />
          )}
        </>
      )}
    </>
  )
}
