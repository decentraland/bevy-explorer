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
import { Minimap } from './features/minimap/Minimap'
import { Pointer } from './features/pointer/Pointer'
import { openPassport } from './features/profile/Passport'
import { openWorldVisit } from './components/WorldVisitModal'
import { openPermissionDialog } from './features/permissions/PermissionDialog'
import { PopupHost } from './design'
import { SessionProvider } from './features/session/SessionContext'
import { FpsMeter } from './features/debug/FpsMeter'
import { LoadingAndLogin } from './features/login/LoadingAndLogin'
import { SceneLoadingOverlay } from './features/session/SceneLoadingOverlay'
import { openExitConfirm } from './features/session/ExitConfirm'
import { useEngineSession } from './features/session/useEngineSession'
import { useExitGuard } from './lib/useExitGuard'
import { useWindowKeyDown } from './lib/useWindowKeyDown'
import { bootMode } from './lib/bootMode'
import { isMobile, isChromiumBased, hasBypassCookie } from './lib/isMobile'
import { hasUsableGpu } from './lib/gpu'
import { MobileGate, GateChecking } from './features/gate/MobileGate'
import { UntrustedLaunchGate } from './features/gate/UntrustedLaunchGate'
import { isTrustedSystemScene } from './lib/systemScene'
import { BASE_DOMAIN, isTrustedBaseDomain } from './lib/baseDomain'
import { ErrorBoundary } from './features/error/ErrorBoundary'
import { CrashModal } from './features/error/CrashModal'
import { openRealmError } from './features/error/RealmErrorModal'

const params = new URLSearchParams(location.search)
// MOCK (?mock=1): UI only, no engine, fake bridge (?previousLogin=1 → returning user).
// ENGINE (default): real engine in a same-document canvas (EngineHost — no iframe) + super-user bridge scene.
// NATIVE (?native=1): HUD in a CEF offscreen webview over the native bevy engine — a JS shim
// bridges this app's BroadcastChannel to the engine's native relay (no iframe, no mock).
const MODE: 'mock' | 'engine' | 'native' =
  params.get('mock') === '1' ? 'mock' : __NATIVE_HUD__ && params.get('native') === '1' ? 'native' : 'engine'
const SHOWCASE = params.get('showcase') === '1'
// Embedded/debug mode (?hud=0 or ?systemScene= — see lib/bootMode.ts): render ONLY the
// engine — no React HUD at all (no sidebar, chat, pointer, panels, or the sign-in /
// loading overlays). Something else owns the UI: the sites `/discover` embed's own
// chrome, or the substituted ui scene in-engine.
const HIDE_HUD = bootMode().hideHud
// Gate: don't mount the HUD/engine where the engine can't run — mobile (no WebGPU/SharedArrayBuffer)
// or a non-Chromium desktop browser (the engine bundle renders its own "Browser Not Supported" page
// there — see deploy/web/index.html — which the HUD would otherwise cover, leaving login frozen at
// 0%). `?gate=1` forces the mobile variant, `?gate=browser` the browser variant, `?gate=gpu` the
// no-GPU variant (all for testing); `?nogate=1` bypasses; the shared `bypass_browser_check` cookie
// ("try anyway") also bypasses. The mobile/browser checks are synchronous (UA); the real no-GPU
// detection is async (see useGpuProbe) — `?gate=gpu` short-circuits it here for a deterministic gate.
function gateReason(): 'mobile' | 'browser' | 'gpu' | null {
  // Precedence: a real device constraint wins over a test override — mobile is checked before
  // `?gate=browser`, so on an actual phone that param still (correctly) yields the mobile variant.
  // Native (CEF webview over native bevy): no browser/mobile gate — the host app IS the platform.
  if (MODE === 'native') return null
  if (params.get('nogate') === '1') return null
  if (isMobile() || params.get('gate') === '1') return 'mobile'
  if (params.get('gate') === 'browser') return 'browser'
  if (params.get('gate') === 'gpu') return 'gpu'
  if (!isChromiumBased() && !hasBypassCookie()) return 'browser'
  return null
}
const GATE_REASON = gateReason()

// `?systemScene=` picks the super-user scene, which permissions.rs waves through every permission
// check and hands the whole SystemApi. A link carrying an unrecognised one is a session takeover
// with no sandbox escape involved, so it gets an interstitial before anything boots — captured at
// module scope, from the ENTRY url, so a later history.replaceState can't retire the warning.
const SYSTEM_SCENE_OVERRIDE = bootMode().systemScene
const UNTRUSTED_SYSTEM_SCENE =
  SYSTEM_SCENE_OVERRIDE != null && !isTrustedSystemScene(SYSTEM_SCENE_OVERRIDE) ? SYSTEM_SCENE_OVERRIDE : null
// `?baseDomain=` likewise: it points every backend at another deployment. Native is exempt —
// there the shell injects it from the user's own --base-domain flag, not from a link.
const UNTRUSTED_BASE_DOMAIN = MODE !== 'native' && !isTrustedBaseDomain(BASE_DOMAIN) ? BASE_DOMAIN : null

export function App(): React.JSX.Element {
  const showFps = useFpsToggle()
  // Probe the GPU before booting the engine — but only on the real boot path (a sync gate already
  // decided, mock, native, or the design showcase all skip it). Native renders through the host's
  // bevy/wgpu, not WebGPU in this webview, so probing there would gate a HUD that runs fine.
  // 'checking' shows a brief spinner.
  const gpu = useGpuProbe(GATE_REASON != null || MODE === 'mock' || MODE === 'native' || SHOWCASE)
  // Ahead of every other gate: returning here is what keeps EngineHost unmounted, and EngineHost is
  // what injects boot.js and starts the scene.
  const [trustUntrusted, setTrustUntrusted] = useState(false)
  if ((UNTRUSTED_SYSTEM_SCENE != null || UNTRUSTED_BASE_DOMAIN != null) && !trustUntrusted) {
    return (
      <UntrustedLaunchGate
        systemScene={UNTRUSTED_SYSTEM_SCENE ?? undefined}
        baseDomain={UNTRUSTED_BASE_DOMAIN ?? undefined}
        onProceed={() => setTrustUntrusted(true)}
      />
    )
  }
  if (GATE_REASON) return <MobileGate reason={GATE_REASON} />
  if (gpu === 'checking') return <GateChecking />
  if (gpu === 'blocked') return <MobileGate reason="gpu" />
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
// (works even when the engine holds keyboard focus — see useWindowKeyDown).
function useFpsToggle(): boolean {
  const [on, setOn] = useState(params.get('fps') === '1')
  useWindowKeyDown((e) => {
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'F' || e.key === 'f')) {
      e.preventDefault()
      setOn((v) => !v)
    }
  })
  return on
}

// Async pre-boot GPU probe. 'checking' → 'ok' (boot) or 'blocked' (→ the no-GPU gate). `skip` (a sync
// gate already decided, mock, native, or showcase) resolves immediately to 'ok'. `?nogate=1` and the shared
// bypass cookie skip the gate too; `?gate=gpu` is handled synchronously in gateReason(), so this only
// runs the real detection path.
function useGpuProbe(skip: boolean): 'checking' | 'ok' | 'blocked' {
  const [state, setState] = useState<'checking' | 'ok' | 'blocked'>(skip ? 'ok' : 'checking')
  useEffect(() => {
    if (skip) return
    if (params.get('nogate') === '1' || hasBypassCookie()) {
      setState('ok')
      return
    }
    let cancelled = false
    void hasUsableGpu().then((ok) => {
      if (!cancelled) setState(ok ? 'ok' : 'blocked')
    })
    return () => {
      cancelled = true
    }
  }, [skip])
  return state
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
    if (MODE === 'native') {
      // The CEF shim wires this BroadcastChannel to the native engine relay (see
      // src/lib/cefNativeBridge.ts and src/react_hud_cef.rs).
      return { rpc: null, createDriver: () => new BridgeClient() }
    }
    const engineRpc = new EngineRpc()
    return { rpc: engineRpc, createDriver: () => new EngineDriver(engineRpc) }
  }, [])

  const session = useEngineSession(createDriver)
  // Warn before the back gesture / Back button unloads the engine (only once in-world). Shown through
  // the popup layer so hasOpenPopup() covers it (Enter must not focus the chat behind it); Escape /
  // scrim-click resolve to "stay", which clears `confirming` and the effect closes the (already-closed) popup.
  const exitGuard = useExitGuard(session.phase === 'entering' || session.phase === 'world')
  useEffect(() => {
    if (!exitGuard.confirming) return
    return openExitConfirm(exitGuard.stay, exitGuard.leave)
  }, [exitGuard.confirming, exitGuard.stay, exitGuard.leave])

  // A world that doesn't exist isn't a crash — it's an ordinary dialog on the popup layer, so it gets
  // Escape/scrim-click for free and freezes nothing behind it (unlike CrashModal, see inputLock). Any
  // close path clears the session's error, so a keyboard dismiss can't strand it.
  const realmErrorMessage = session.fatalError?.source === 'realm' ? session.fatalError.message : null
  useEffect(() => {
    if (realmErrorMessage == null) return
    return openRealmError({ message: realmErrorMessage, onDismiss: session.dismissFatal })
    // eslint-disable-next-line react-hooks/exhaustive-deps -- re-open only when the message changes
  }, [realmErrorMessage])
  // Everything else IS a crash → the full-screen CrashModal (narrowed so it never sees 'realm').
  const crash =
    session.fatalError != null && session.fatalError.source !== 'realm'
      ? { message: session.fatalError.message, source: session.fatalError.source }
      : null

  // Scene permission prompts (e.g. ChangeRealm), one at a time, opened through the popup layer. Escape
  // / scrim-click deny once. Re-opens whenever the front of the pending queue changes.
  const permFrontId = session.phase === 'world' ? session.permissions.pending[0]?.id : undefined
  useEffect(() => {
    const front = session.permissions.pending[0]
    if (permFrontId == null || !front) return
    return openPermissionDialog(front, session.permissions.resolve)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- re-open only when the front request changes
  }, [permFrontId])

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

  // Embedded mode: mount only the engine (+ the error surfaces so a crash isn't silently blank).
  // No sidebar / chat / pointer / panels / sign-in UI. PopupHost renders nothing while the stack is
  // empty, and it's what a world-not-found dialog opens into — without it that error would be invisible.
  if (HIDE_HUD) {
    return (
      <SessionProvider value={session}>
        {rpc && <EngineHost rpc={rpc} />}
        <PopupHost />
        {/* After PopupHost: the crash surface sits above the popup layer. */}
        {crash && <CrashModal error={crash} onReload={session.reload} onDismiss={crash.source === 'runtime' ? session.dismissFatal : undefined} />}
      </SessionProvider>
    )
  }

  return (
    <SessionProvider value={session}>
      {rpc && <EngineHost rpc={rpc} />}
      {session.phase === 'login' && <LoadingAndLogin flow={session.login} />}
      {session.phase === 'picking' && <PlacesPicker onPick={session.pickDestination} />}
      {session.phase === 'entering' && <SceneLoadingOverlay scene={session.sceneLoading} />}
      {session.phase === 'world' && !session.menuOpen && (
        <>
          {/* The full-screen menu pages own the whole screen; hide the rail + chat so
              they don't show through (the map page's body is transparent). */}
          {!pageOpen && <Sidebar session={session} onViewProfile={viewMyProfile} />}
          {!pageOpen && (
            <Minimap
              minimap={session.minimap}
              map={session.map}
              sceneTitle={session.minimap.sceneTitle}
              setEngineViewport={session.setEngineViewport}
            />
          )}
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
            onVisitWorld={(name) => openWorldVisit({ worldName: name, onConfirm: () => session.map.changeRealm(name) })}
          />
          <FriendsPanel friends={session.friends} />
          <SettingsPanel settings={session.settings} bindings={session.bindings} profile={session.profile} onNavigate={goToMenuPage} />
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
        </>
      )}
      {/* Popups (imperative overlay stack) live inside the session provider so popup-mounted surfaces
          — the world <ProfileCard> — can read useSession(). */}
      <PopupHost />
      {/* Crash surface (boot panic / runtime crash / react render crash) — last, so it renders above
          the popup layer. A world-not-found is not a crash and opens as a popup instead (see above). */}
      {crash && <CrashModal error={crash} onReload={session.reload} onDismiss={crash.source === 'runtime' ? session.dismissFatal : undefined} />}
    </SessionProvider>
  )
}
