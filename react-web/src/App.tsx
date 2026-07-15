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
import { openPassport } from './features/profile/Passport'
import { WorldVisitModal } from './components/WorldVisitModal'
import { PermissionDialog } from './features/permissions/PermissionDialog'
import { PopupHost } from './design'
import { SessionProvider } from './features/session/SessionContext'
import { FpsMeter } from './features/debug/FpsMeter'
import { LoadingAndLogin } from './features/login/LoadingAndLogin'
import { SceneLoadingOverlay } from './features/session/SceneLoadingOverlay'
import { ExitConfirm } from './features/session/ExitConfirm'
import { useEngineSession } from './features/session/useEngineSession'
import { useExitGuard } from './lib/useExitGuard'
import { useHudScale } from './lib/useHudScale'
import { useGlobalHotkey } from './lib/useGlobalHotkey'
import { useMenuShortcuts } from './lib/useMenuShortcuts'
import { isMobile, isChromiumBased, hasBypassCookie } from './lib/isMobile'
import { MobileGate } from './features/gate/MobileGate'
import { ErrorBoundary } from './features/error/ErrorBoundary'
import { EngineErrorModal } from './features/error/EngineErrorModal'

const params = new URLSearchParams(location.search)
// MOCK (?mock=1): UI only, no engine, fake bridge (?previousLogin=1 → returning user).
// ENGINE (default): real engine in a same-origin iframe + super-user bridge scene.
const MODE: 'mock' | 'engine' = params.get('mock') === '1' ? 'mock' : 'engine'
const SHOWCASE = params.get('showcase') === '1'
// Embedded mode (?hud=0): render ONLY the engine — no React HUD at all (no
// sidebar, chat, pointer, panels, or the sign-in / loading overlays). Used by
// the sites `/discover` embed, which frames the scene itself and supplies its
// own chrome. Pair with ?guest=1 (auto guest-login) so the engine still enters
// the world without the — now hidden — sign-in screen.
const HIDE_HUD = params.get('hud') === '0'
// Gate: don't mount the HUD/engine where the engine can't run — mobile (no WebGPU/SharedArrayBuffer)
// or a non-Chromium desktop browser (the engine bundle renders its own "Browser Not Supported" page
// there — see deploy/web/index.html — which the HUD would otherwise cover, leaving login frozen at
// 0%). `?gate=1` forces the mobile variant, `?gate=browser` the browser variant (both for testing);
// `?nogate=1` bypasses; the shared `bypass_browser_check` cookie ("try anyway") also bypasses.
function gateReason(): 'mobile' | 'browser' | null {
  // Precedence: a real device constraint wins over a test override — mobile is checked before
  // `?gate=browser`, so on an actual phone that param still (correctly) yields the mobile variant.
  if (params.get('nogate') === '1') return null
  if (isMobile() || params.get('gate') === '1') return 'mobile'
  if (params.get('gate') === 'browser') return 'browser'
  if (!isChromiumBased() && !hasBypassCookie()) return 'browser'
  return null
}
const GATE_REASON = gateReason()

export function App(): React.JSX.Element {
  useHudScale() // keep --ui-scale in sync with the viewport (DPI-correct, like Unity)
  const showFps = useFpsToggle()
  if (GATE_REASON) return <MobileGate reason={GATE_REASON} />
  return (
    <ErrorBoundary>
      {SHOWCASE ? (
        <Suspense fallback={null}>
          <Showcase />
        </Suspense>
      ) : (
        <Hud />
      )}
      {showFps && <FpsMeter />}
    </ErrorBoundary>
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
  // Warn before the back gesture / Back button unloads the engine (only once in-world).
  const exitGuard = useExitGuard(session.phase === 'entering' || session.phase === 'world')

  // A world (e.g. boedo.dcl.eth) the user asked to jump to — drives the shared confirm modal.
  const [visitWorld, setVisitWorld] = useState<string | null>(null)
  // Which tab the Backpack opens on. The emote wheel's "Customise [E]" opens it on Emotes; it resets
  // to Wearables once the Backpack closes so a normal (sidebar/topbar) open lands on Wearables.
  const [backpackTab, setBackpackTab] = useState<'wearables' | 'emotes'>('wearables')
  useEffect(() => {
    if (!session.backpack.open) setBackpackTab('wearables')
  }, [session.backpack.open])
  // Open MY OWN passport (Sidebar profile icon + the menu's "View Profile") — same popup the profile
  // card opens for others.
  const viewMyProfile = (): void => {
    const me = session.profile.data
    if (me) openPassport(me.address)
  }

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

  // Embedded mode: mount only the engine (+ the fatal-error surface so a crash
  // isn't silently blank). No sidebar / chat / pointer / panels / sign-in UI.
  if (HIDE_HUD) {
    return (
      <SessionProvider value={session}>
        {rpc && <EngineHost rpc={rpc} />}
        {session.fatalError && (
          <EngineErrorModal
            error={session.fatalError}
            onReload={session.reload}
            onDismiss={session.fatalError.source === 'runtime' || session.fatalError.source === 'realm' ? session.dismissFatal : undefined}
          />
        )}
      </SessionProvider>
    )
  }

  return (
    <SessionProvider value={session}>
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
          {!pageOpen && (
            <Pointer
              hover={session.hover}
              locked={session.cursorLocked}
              proximity={session.proximity}
            />
          )}
          <Chat
            chat={session.chat}
            hidden={session.friends.open || pageOpen}
            me={session.profile.data}
            onTeleport={(x, y) => session.map.teleport(x, y)}
            onVisitWorld={(name) => setVisitWorld(name)}
          />
          <FriendsPanel friends={session.friends} />
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
            onViewProfile={(u) => openPassport(u.address)}
          />
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
      {/* Scene permission prompts (e.g. ChangeRealm) — one at a time, above any open menu. */}
      {session.phase === 'world' && session.permissions.pending.length > 0 && (
        <PermissionDialog
          key={session.permissions.pending[0].id}
          request={session.permissions.pending[0]}
          onResolve={(allow, level) =>
            session.permissions.resolve(session.permissions.pending[0].id, allow, level)
          }
        />
      )}
      {/* Confirm before the back gesture / Back button unloads the engine. */}
      {exitGuard.confirming && <ExitConfirm onStay={exitGuard.stay} onLeave={exitGuard.leave} />}
      {/* Fatal engine error (boot panic / runtime crash) — above everything. */}
      {session.fatalError && (
        <EngineErrorModal
          error={session.fatalError}
          onReload={session.reload}
          onDismiss={session.fatalError.source === 'runtime' || session.fatalError.source === 'realm' ? session.dismissFatal : undefined}
        />
      )}
      {/* Popups (imperative overlay stack) live inside the session provider so popup-mounted surfaces
          — the world <ProfileCard> — can read useSession(). */}
      <PopupHost />
    </SessionProvider>
  )
}
