// Top-level session orchestration: login → entering (scene loading) → world.
// Owns the driver and exposes the login flow + scene-loading state + phase.

import { useCallback, useEffect, useRef, useState } from 'react'
import { clearStoredLogins, getStoredLogin, redirectToAuth, rootAddress, type StoredLogin } from '../auth/sso'
import type { LoginDriver } from '../../engine/driver'
import type { FatalError } from '../error/EngineErrorModal'
import { DEFAULT_REALM } from '../engine/EngineHost'
import { closeTopPopup, hasOpenPopup } from '../../design'
import { getCursor } from '../pointer/cursorStore'
import { openProfileCard } from '../profileCard/ProfileCard'
import { parseChatCommand } from '../chat/chatCommands'
import type {
  AppNotification,
  ChatMessage,
  Community,
  CommunityDetailMessage,
  Emote,
  Friend,
  FriendAction,
  FriendRequest,
  GalleryPhoto,
  GalleryPhotoMeta,
  HoverAction,
  NavAction,
  NearbyMember,
  PermissionLevelChoice,
  PermissionRequestMessage,
  ProximityTip,
  Profile,
  SceneLoadingState,
  Setting,
  Wearable
} from '../../engine/protocol'

export interface BackpackState {
  list: Wearable[]
  open: boolean
  toggle: () => void
  /** Persist a full equipped set to the profile (the explicit Equip action). */
  equip: (urns: string[]) => void
  /** Preview a set on the avatar without persisting (selecting); null reverts to the profile. */
  preview: (urns: string[] | null) => void
}

export interface CommunitiesState {
  list: Community[]
  open: boolean
  toggle: () => void
  /** Create a community (name + description + Public/Private + discoverable). */
  create: (input: { name: string; description: string; privacy: 'public' | 'private'; discoverable: boolean }) => void
  join: (id: string) => void
  leave: (id: string) => void
  /** Per-community detail (members/posts/places/events) for the open modal. */
  detail: CommunityDetailMessage | null
  /** Request a community's detail (call when its modal opens). */
  loadDetail: (id: string) => void
}

export interface MapState {
  /** Local player's current parcel. */
  x: number
  y: number
  open: boolean
  toggle: () => void
  teleport: (x: number, y: number) => void
  /** Travel to a world/realm by name (e.g. `boedo.dcl.eth`). */
  changeRealm: (realm: string) => void
}

// Places browses the places API over HTTP (no bridge data) — it only needs open/close.
export interface PlacesState {
  open: boolean
  toggle: () => void
}

export interface GalleryState {
  /** The local player's camera-reel photos (newest first by dateTime). */
  list: GalleryPhoto[]
  /** Storage usage — `current` of `max` photos (0 until the gallery first loads). */
  current: number
  max: number
  /** False until the first gallery response arrives (spinner vs empty state). */
  loaded: boolean
  open: boolean
  toggle: () => void
  /** Per-photo metadata cache (place + people), filled lazily when a detail view opens. */
  metas: Record<string, GalleryPhotoMeta | null | undefined>
  /** Fetch one photo's full metadata (populates `metas`). */
  loadPhoto: (id: string) => void
  /** Delete one of the player's photos (re-emits the gallery). */
  remove: (id: string) => void
}

export interface EmotesState {
  list: Emote[]
  open: boolean
  toggle: () => void
  play: (urn: string) => void
  /** Assign an owned emote to a wheel slot (0–9); urn:'' clears the slot. */
  equip: (slot: number, urn: string) => void
}

export interface NotificationsState {
  list: AppNotification[]
  unread: number
  open: boolean
  toggle: () => void
  markAllRead: () => void
}

export interface SettingsState {
  list: Setting[]
  open: boolean
  toggle: () => void
  set: (name: string, value: number) => void
}

export interface ProfileState {
  data: Profile | null
  open: boolean
  toggle: () => void
}

export interface FriendsState {
  /** false for guests / before the relationship snapshot is seeded. */
  available: boolean
  list: Friend[]
  received: FriendRequest[]
  sent: FriendRequest[]
  blocked: string[]
  open: boolean
  toggle: () => void
  /* TODO: split domain data (queries) from commands — act/toggle don't belong in "State".
   Expose commands as an imperative service/context (like the popup service), not prop-drilled. (#18) */
  /** accept/reject/cancel/delete/block/unblock a user (guest-disabled in-engine). */
  act: (op: FriendAction, address: string) => void
}

export interface PermissionsState {
  /** Outstanding scene permission prompts, oldest first (the HUD shows one at a time). */
  pending: PermissionRequestMessage[]
  /** Allow/deny the request `id` at the chosen scope, then drop it from the queue. */
  resolve: (id: number, allow: boolean, level: PermissionLevelChoice) => void
}

export type ChatLine = ChatMessage & { id: number; ts: number }

export interface ChatState {
  messages: ChatLine[]
  send: (text: string) => void
  /** Visibility, toggled by the React sidebar chat icon. */
  open: boolean
  toggle: () => void
  /** Nearby players (drives the "Nearby · N" header + members list). */
  members: NearbyMember[]
  /** Open chat and queue an @name mention into the draft (from a profile card's "Mention"). */
  mention: (name: string) => void
  /** A queued @name waiting to be dropped into the chat draft (consumed by Chat), or null. */
  pendingMention: string | null
  /** Clear the queued mention once Chat has inserted it. */
  consumeMention: () => void
  /** Messages received while closed, reset to 0 on open (drives the sidebar badge). */
  unread: number
  /** Bumped on every engine "focus chat" request (Enter, even while idle-open) — Chat watches
   *  this to (re)focus the input beyond the open-transition case. */
  focusTick: number
  /** Open + (re)focus chat. Called by useMenuShortcuts' page-level Enter handler for when DOM
   *  focus is on some other HUD element (a button would otherwise just activate on Enter). */
  requestFocus: () => void
}

const MAX_CHAT_LINES = 200

// Render-settle: after a scene first reports loaded, hold the loader at least this long for the
// engine to render the first frame (so the world isn't revealed as black models), and at most this
// long so a stuck/absent render probe never traps the loader. The cap is deliberately short: the
// `renderBusy` probe is best-effort (the engine build may not even expose `#shader-compiling`), so a
// long cap turned a missed probe into a multi-second frozen-looking hold with the engine idle.
const MIN_REVEAL_MS = 300
const MAX_REVEAL_MS = 1500
// While loading, the engine streams scenes and `visible` can briefly drop between one finishing
// and the next starting. Revealing the world in that gap flashes the HUD (chat + sidebar) in and
// out, so we only drop the loader once loading has been stably clear for this long.
const REVEAL_DEBOUNCE_MS = 600

export type LoginStatus =
  | 'loading'
  | 'sign-in-or-guest'
  | 'reuse-login-or-new'

export type SessionPhase = 'login' | 'picking' | 'entering' | 'world'

// Where the user chose to spawn after login (the post-jump-in Places picker). `null` = skip → the
// engine's default spawn (Genesis Plaza). A world switches realm; a parcel teleports once spawned.
export type Destination =
  | { kind: 'parcel'; x: number; y: number }
  | { kind: 'world'; realm: string; position?: string }
  | null

export interface LoginFlow {
  status: LoginStatus
  /** Root wallet address of the stored SSO identity (shown on the "welcome back" screen). */
  account: string | null
  busy: boolean
  error: string | null
  /** Engine has booted and can accept the login command. The engine-driven CTAs (Jump in / Explore)
   *  stay disabled until this is true so a click never lands in a silent wait. Always true for mock. */
  engineReady: boolean
  /** Real boot progress (0–100, weighted) from the engine loader, for the login footer bar. */
  loadProgress: number
  /** Active boot step id ('download'|'compile'|'init'|'workers'|'gpu') or null. */
  loadStep: string | null
  /** Fresh sign-in: same-domain auth redirect (web) or the engine's remote-wallet flow (native). */
  startWithAccount: () => void
  /** Native fresh sign-in in flight → show the verification panel (code null until it arrives). */
  authPending: boolean
  authCode: string | null
  /** Abort the in-flight native fresh sign-in. */
  cancelLogin: () => void
  exploreAsGuest: () => void
  /** Reuse the stored SSO identity (hand it to the engine). */
  jumpIn: () => void
  /** The last login failed because the user's profile couldn't be fetched — offer
   *  resetProfileAndJumpIn as the explicit recovery. */
  profileFetchFailed: boolean
  /** Retry the login, continuing with (and deploying) a default profile if the fetch still
   *  fails. PERMANENTLY REPLACES the account's existing profile — only offered after a
   *  profile-fetch failure, and the UI must make the consequence clear. */
  resetProfileAndJumpIn: () => void
  /** Sign in with a different account → auth site. */
  useDifferentAccount: () => void
}

export interface EngineSession {
  phase: SessionPhase
  /** Post-jump-in Places picker: choose where to spawn (or null to skip → Genesis Plaza). */
  pickDestination: (dest: Destination) => void
  login: LoginFlow
  scene: SceneLoadingState | null
  /** Fatal engine error → full-screen error popup. 'launch' = boot panic (fatal), 'runtime' =
   *  post-launch crash bridged from the engine watchdog (dismissable). null when healthy. */
  fatalError: FatalError | null
  /** Reload the whole page (error-popup action). */
  reload: () => void
  /** Dismiss a non-fatal (runtime) error popup. */
  dismissFatal: () => void
  /** World-entity hover hints (empty = nothing hovered). */
  hover: HoverAction[]
  /** Engine has grabbed the mouse for camera-look (OS cursor hidden) → show the crosshair. */
  cursorLocked: boolean
  /** In-range world-entity tooltips, anchored at projected screen coords. */
  proximity: ProximityTip[]
  chat: ChatState
  friends: FriendsState
  settings: SettingsState
  profile: ProfileState
  /** Fetched OTHER-user passports (View Profile), keyed by lowercased address. */
  userProfiles: Record<string, Profile | null>
  /** Request a user's passport by address (populates `userProfiles`). */
  requestUserProfile: (address: string) => void
  notifications: NotificationsState
  emotes: EmotesState
  backpack: BackpackState
  communities: CommunitiesState
  map: MapState
  places: PlacesState
  gallery: GalleryState
  /** Scene permission prompts (e.g. ChangeRealm) awaiting an Allow/Deny. */
  permissions: PermissionsState
  mic: { enabled: boolean; available: boolean; toggle: () => void }
  /** Trigger a sidebar nav action in the scene (open menu/popup, emotes, mic). */
  nav: (action: NavAction) => void
  /** Report (or clear) the screen rect where the scene should render an engine view. */
  setEngineViewport: (region: 'map' | 'avatarPreview', rect: { x: number; y: number; width: number; height: number } | null) => void
  /** Sign out → back to the login screen. */
  logout: () => void
  /** A full scene menu page is open → the React HUD (sidebar + chat) hides. */
  menuOpen: boolean
}

/** Parse a camera-reel `dateTime` (unix seconds, unix ms, or ISO) to epoch ms for sort/grouping. */
export function photoTime(dateTime: string): number {
  if (/^\d+$/.test(dateTime)) {
    const n = Number(dateTime)
    return n < 1e12 ? n * 1000 : n // sub-1e12 → seconds, else already ms
  }
  const t = Date.parse(dateTime)
  return Number.isNaN(t) ? 0 : t
}

export function useEngineSession(createDriver: () => LoginDriver): EngineSession {
  const driverRef = useRef<LoginDriver | null>(null)
  const [status, setStatus] = useState<LoginStatus>('loading')
  // Same-domain SSO identity read from localStorage (null = no stored account).
  const [stored, setStored] = useState<StoredLogin | null>(null)
  // The address of the engine's actual reusable previous login (drives "Welcome back").
  const [prevUserId, setPrevUserId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Engine boots (autostart) while the login screen is up; this flips true once it can take commands.
  const [engineReady, setEngineReady] = useState(false)
  // Real WASM-download/boot progress surfaced from the engine iframe (0–100) + the active step id,
  // for the login footer bar. The engine's own loader is hidden (hideLoader=1).
  const [loadProgress, setLoadProgress] = useState(0)
  const [loadStep, setLoadStep] = useState<string | null>(null)
  // Fatal engine error → full-screen popup. 'launch' = boot panic (fatal, no dismiss); 'runtime' =
  // post-launch crash bridged from the engine watchdog (can be a false positive → dismissable).
  // ?simerror=1 (or =launch) seeds a sample so the popup can be iterated without a real panic.
  const [fatalError, setFatalError] = useState<FatalError | null>(() => {
    const sim = new URLSearchParams(location.search).get('simerror')
    if (sim == null) return null
    return {
      message: "panicked at crates/dcl_wasm/src/inner/mod.rs:41:9:\ncan't init wasm queue\n\n(simulated)",
      source: sim === 'launch' ? 'launch' : 'runtime'
    }
  })

  // Past login → waiting for the world.
  const [submitted, setSubmitted] = useState(false)
  // The post-jump-in Places picker: stay in 'picking' until the user chooses a destination (or skips).
  const [destinationPicked, setDestinationPicked] = useState(false)
  // Deferred login: the login call captured on Jump in, run only once the user picks a destination
  // (so the engine is launched straight at that destination instead of loading Genesis Plaza first).
  const pendingLogin = useRef<((driver: LoginDriver) => Promise<unknown>) | null>(null)
  // Native fresh sign-in (driver.loginNew) in flight: non-null shows the verification-code panel;
  // `code` fills in when the engine's 'loginCode' message lands. The attempt counter invalidates
  // a cancelled attempt's eventual resolution (the promise settles after loginCancel).
  const [auth, setAuth] = useState<{ code: string | null } | null>(null)
  const authAttempt = useRef(0)
  // Stops the post-launch boot-panic poll once the world is reached (so it can't mislabel a benign
  // post-boot panic as a launch failure). The timer id is kept so it's cancelled on unmount.
  const bootPollStop = useRef(false)
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  // Native parcel pick: the engine can only teleport an existing player (there's no boot-at-position
  // when the engine is already running), so the pick is held until playerReady — or sent at once if
  // the player already spawned (see the no-launch pick path).
  const pendingParcel = useRef<{ x: number; y: number } | null>(null)
  const [playerReady, setPlayerReady] = useState(false)
  // Ref twin of playerReady: the destination pick runs in a callback that would close over a
  // stale value of the state.
  const playerReadyRef = useRef(false)
  const [scene, setScene] = useState<SceneLoadingState | null>(null)
  const [hover, setHover] = useState<HoverAction[]>([])
  const [proximity, setProximity] = useState<ProximityTip[]>([])
  const [cursorLocked, setCursorLocked] = useState(false)
  const [messages, setMessages] = useState<ChatLine[]>([])
  const [members, setMembers] = useState<NearbyMember[]>([])
  // Mirror cursor-lock into a ref so the run-once message handler reads it without a stale closure —
  // avatarClick uses it to centre the card while the camera has the pointer locked.
  const cursorLockedRef = useRef(false)
  const [chatOpen, setChatOpen] = useState(true)
  const [chatUnread, setChatUnread] = useState(0)
  const [chatFocusTick, setChatFocusTick] = useState(0)
  // Read inside the message-subscription closure (mounted once), not via a stale `chatOpen` capture.
  const chatOpenRef = useRef(chatOpen)
  chatOpenRef.current = chatOpen
  // True while something is covering the chat (see the assignment below for what counts). Read by
  // requestFocusChat from a callback that would otherwise close over a stale value. Popups aren't in
  // here — they live in their own module store, so requestFocusChat asks it directly.
  const chatCoveredRef = useRef(false)
  // Open + (re)focus chat — the engine's "Chat" system action (Enter while the engine holds focus)
  // and the page-level Enter shortcut (Enter while some other HUD element has focus, see
  // useMenuShortcuts) both funnel into this single action.
  const requestFocusChat = useCallback(() => {
    // Enter is only a chat key when nothing is covering the chat. While the main menu, a popup or a
    // modal is up it owns the screen, so Enter neither dismisses it nor focuses the chat behind it.
    // hasOpenPopup() is read here, not during render: the popup stack is a module store that changes
    // without re-rendering this hook.
    if (chatCoveredRef.current || hasOpenPopup()) return
    // Release the browser pointer lock so the mouse stops driving the camera and you can type. On web
    // camera-look IS the pointer lock; exiting it here (the always-firing focus path) reliably releases
    // it, and the engine self-heals — update_pointer_lock drops camera-look on
    // `!document.pointerLockElement`. No-op on native (no DOM pointer lock; the engine frees its own OS
    // cursor grab from the Chat action).
    document.exitPointerLock?.()
    // Friends is the only panel Enter closes: it shares the chat's bottom-left dock, so Chat renders
    // null behind it (App's `hidden`) and focusing without closing it would do nothing on screen.
    // The rest (profile, notifications, emotes) sit clear of the chat, so leave them open.
    setFriendsOpen(false)
    setChatOpen(true)
    setChatFocusTick((t) => t + 1)
  }, [])
  const [pendingMention, setPendingMention] = useState<string | null>(null)
  const [menuOpen, setMenuOpen] = useState(false)
  const [friendsData, setFriendsData] = useState<{
    available: boolean
    friends: Friend[]
    received: FriendRequest[]
    sent: FriendRequest[]
    blocked: string[]
  }>({ available: false, friends: [], received: [], sent: [], blocked: [] })
  const [friendsOpen, setFriendsOpen] = useState(false)
  const [settings, setSettings] = useState<Setting[]>([])
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [profile, setProfile] = useState<Profile | null>(null)
  const [profileOpen, setProfileOpen] = useState(false)
  // Fetched OTHER-user passports (View Profile), keyed by lowercased address.
  const [userProfiles, setUserProfiles] = useState<Record<string, Profile | null>>({})
  const [notifications, setNotifications] = useState<AppNotification[]>([])
  const [notificationsOpen, setNotificationsOpen] = useState(false)
  const [emotes, setEmotes] = useState<Emote[]>([])
  const [emotesOpen, setEmotesOpen] = useState(false)
  const [mic, setMic] = useState({ enabled: false, available: false })
  const [wearables, setWearables] = useState<Wearable[]>([])
  const [backpackOpen, setBackpackOpen] = useState(false)
  const [communities, setCommunities] = useState<Community[]>([])
  const [communitiesOpen, setCommunitiesOpen] = useState(false)
  const [communityDetail, setCommunityDetail] = useState<CommunityDetailMessage | null>(null)
  const [mapParcel, setMapParcel] = useState({ x: 0, y: 0 })
  const [mapOpen, setMapOpen] = useState(false)
  const [placesOpen, setPlacesOpen] = useState(false)
  const [galleryPhotos, setGalleryPhotos] = useState<GalleryPhoto[]>([])
  const [galleryStorage, setGalleryStorage] = useState({ current: 0, max: 0 })
  const [galleryLoaded, setGalleryLoaded] = useState(false)
  const [galleryMetas, setGalleryMetas] = useState<Record<string, GalleryPhotoMeta | null>>({})
  const [galleryOpen, setGalleryOpen] = useState(false)
  const [permissionQueue, setPermissionQueue] = useState<PermissionRequestMessage[]>([])
  const chatId = useRef(0)
  // Catalog fetches done once per session (cache; relays re-emit on change).
  const fetchedRef = useRef<Set<string>>(new Set())

  useEffect(() => {
    const driver = createDriver()
    driverRef.current = driver

    // One generic subscription; switch on kind (mirrors dcl-editor's onSceneMessage).
    const off = driver.on((msg) => {
      switch (msg.kind) {
        case 'event':
          if (msg.name === 'playerReady') {
            playerReadyRef.current = true
            setPlayerReady(true)
            bootPollStop.current = true // world reached → stop watching for a boot panic
            if (pendingParcel.current != null) {
              driver.send({ kind: 'teleport', ...pendingParcel.current })
              pendingParcel.current = null
            }
          }
          break
        case 'loginCode':
          // Only meaningful while a fresh sign-in is in flight; a stray late code must not
          // resurrect the panel.
          setAuth((a) => (a == null ? a : { code: msg.code }))
          break
        case 'sceneLoading':
          setScene(msg.state)
          break
        case 'hover':
          setHover(msg.actions)
          break
        case 'cursorLock':
          cursorLockedRef.current = msg.locked
          setCursorLocked(msg.locked)
          break
        case 'systemAction':
          // 'Cancel' (Escape, from the engine input stream) closes the topmost popup — authoritative,
          // so it works even while the engine holds keyboard focus.
          if (msg.action === 'Cancel') closeTopPopup()
          break
        case 'proximity':
          setProximity(msg.tips)
          break
        case 'avatarClick': {
          // The card's scrim swallows mouse input, so the engine's raycast freezes and never sends
          // the hover-exit — clear the hover here or its tooltip stays painted beside the card.
          setHover([])
          // Open the profile card as a popup, anchored at the live DOM cursor (centre while the camera
          // has the pointer locked). The card resolves the name/avatar by address from the roster.
          const p = cursorLockedRef.current ? { x: window.innerWidth / 2, y: window.innerHeight / 2 } : getCursor()
          openProfileCard(msg.address, p.x, p.y)
          break
        }
        case 'chat':
          setMessages((prev) =>
            [...prev, { ...msg.chat, id: chatId.current++, ts: Date.now() }].slice(
              -MAX_CHAT_LINES
            )
          )
          if (!chatOpenRef.current) setChatUnread((n) => n + 1)
          break
        case 'focusChat':
          requestFocusChat()
          break
        case 'chatVisibility':
          setChatOpen(msg.open)
          break
        case 'members':
          setMembers(msg.members)
          break
        case 'menuVisibility':
          setMenuOpen(msg.open)
          break
        case 'friends':
          setFriendsData({
            available: msg.available,
            friends: msg.friends,
            received: msg.received,
            sent: msg.sent,
            blocked: msg.blocked
          })
          break
        case 'settings':
          setSettings(msg.settings)
          break
        case 'profile':
          setProfile(msg.profile)
          break
        case 'userProfile':
          setUserProfiles((prev) => ({ ...prev, [msg.address.toLowerCase()]: msg.profile }))
          break
        case 'notifications':
          setNotifications(msg.notifications)
          break
        case 'emotes':
          setEmotes(msg.emotes)
          break
        case 'mic':
          setMic({ enabled: msg.enabled, available: msg.available })
          break
        case 'wearables':
          setWearables(msg.wearables)
          break
        case 'communities':
          setCommunities(msg.communities)
          break
        case 'communityDetail':
          setCommunityDetail(msg)
          break
        case 'mapState':
          setMapParcel({ x: msg.x, y: msg.y })
          break
        case 'gallery':
          setGalleryPhotos([...msg.photos].sort((a, b) => photoTime(b.dateTime) - photoTime(a.dateTime)))
          setGalleryStorage({ current: msg.current, max: msg.max })
          setGalleryLoaded(true)
          break
        case 'galleryPhoto':
          setGalleryMetas((prev) => ({ ...prev, [msg.id]: msg.meta }))
          break
        case 'permissionRequest':
          setPermissionQueue((q) => (q.some((r) => r.id === msg.id) ? q : [...q, msg]))
          break
      }
    })

    // Same-domain SSO: an identity in this origin's localStorage (written by the auth site)
    // means the user is already signed in — no engine query, no polling. Returning from the
    // auth-site redirect lands here too, with the identity already present.
    // Show "Jump in" only when the ENGINE has a usable previous login. Gating on the stored SSO
    // identity alone showed a Jump-in button that couldn't actually log in (the engine reuses its
    // own saved login via loginPrevious; there is no log-in-with-raw-identity surface), so a stale
    // localStorage entry would strand the user on a button that throws. The driver folds both
    // signals together (engine saved login + SSO) into getPreviousLogin().
    const login = getStoredLogin()
    setStored(login)
    driver
      .getPreviousLogin()
      .then((r) => {
        setPrevUserId(r.userId)
        setStatus(r.userId ? 'reuse-login-or-new' : 'sign-in-or-guest')
      })
      .catch(() => setStatus(login ? 'reuse-login-or-new' : 'sign-in-or-guest'))

    return () => {
      off()
      driver.dispose()
      driverRef.current = null
    }
    // createDriver is stable; run-once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Watch engine boot: the iframe autostarts on mount, so poll until it can take commands, then stop.
  // Drivers without an engine (mock/tests) report ready immediately.
  useEffect(() => {
    const driver = driverRef.current
    if (driver == null || typeof driver.engineReady !== 'function') {
      setEngineReady(true)
      setLoadProgress(100)
      return
    }
    if (driver.engineReady()) {
      setEngineReady(true)
      setLoadProgress(100)
      return
    }
    const id = setInterval(() => {
      const d = driverRef.current
      setLoadProgress(d?.loadProgress?.() ?? 0)
      setLoadStep(d?.loadStep?.() ?? null)
      if (d?.engineReady?.() === true) {
        setEngineReady(true)
        setLoadProgress(100)
        setLoadStep(null)
        clearInterval(id)
      }
    }, 200)
    return () => clearInterval(id)
  }, [])

  // Runtime engine crashes (heartbeat stall) are detected by the bundle's watchdog and bridged to us
  // as a same-origin postMessage (the iframe's own overlay is hidden behind react-web's HUD).
  useEffect(() => {
    // Same-document engine (no iframe): the crash watchdog in engine/boot.js calls this directly
    // instead of rendering any overlay — React owns the error UI.
    const w = window as Window & { __onEngineCrash?: (message: string, source: string) => void }
    w.__onEngineCrash = (message) => {
      setFatalError((prev) => prev ?? { message: message || 'The engine stopped responding.', source: 'runtime' })
    }
    return () => {
      delete w.__onEngineCrash
    }
  }, [])

  // Cancel the boot-panic poll on unmount — the one self-scheduling timer in this hook.
  useEffect(() => () => { if (pollTimer.current) clearTimeout(pollTimer.current) }, [])

  // On world-entry, pull the profile (top-bar chip) and notifications (so the unread badge
  // shows immediately, not only after the panel is first opened). Marked fetched once.
  useEffect(() => {
    if (!playerReady) return
    if (!fetchedRef.current.has('getProfile')) {
      fetchedRef.current.add('getProfile')
      driverRef.current?.send({ kind: 'getProfile' })
    }
    if (!fetchedRef.current.has('getNotifications')) {
      fetchedRef.current.add('getNotifications')
      driverRef.current?.send({ kind: 'getNotifications' })
    }
  }, [playerReady])

  // Inject a local "DCL System" line (empty sender → rendered as the system member) for command
  // feedback (/help, /goto usage, /commands output) that must NOT be broadcast to other players.
  const pushSystemMessage = useCallback((message: string) => {
    setMessages((prev) =>
      [...prev, { sender: '', message, channel: 'Nearby', id: chatId.current++, ts: Date.now() }].slice(-MAX_CHAT_LINES)
    )
  }, [])

  // Chat send doubles as the slash-command interceptor (parity with bevy-ui-scene's `sendChatMessage`):
  // a recognized `/command` never reaches other players — it teleports, reloads, runs an engine console
  // command, or echoes a system message. Anything else is sent as a normal Nearby message.
  const sendChat = useCallback(
    (text: string) => {
      const cmd = parseChatCommand(text)
      switch (cmd.kind) {
        case 'send':
          if (cmd.text) driverRef.current?.send({ kind: 'sendChat', message: cmd.text, channel: 'Nearby' })
          break
        case 'goto':
          driverRef.current?.send({ kind: 'teleport', x: cmd.x, y: cmd.y })
          break
        case 'genesis':
          driverRef.current?.send({ kind: 'changeRealm', realm: DEFAULT_REALM })
          break
        case 'world':
          driverRef.current?.send({ kind: 'changeRealm', realm: cmd.realm })
          break
        case 'reload':
          driverRef.current?.send({ kind: 'reloadScene' })
          break
        case 'commands':
          driverRef.current?.send({ kind: 'consoleCommand', command: 'help' })
          break
        case 'system':
          pushSystemMessage(cmd.message)
          break
      }
    },
    [pushSystemMessage]
  )

  // Toggle one exclusive panel (closing chat + all others); optionally run onOpen.
  // All exclusive (one-at-a-time) panel setters. Toggling one closes chat + the rest.
  const panelSetters = [setFriendsOpen, setSettingsOpen, setProfileOpen, setNotificationsOpen, setEmotesOpen, setBackpackOpen, setCommunitiesOpen, setMapOpen, setPlacesOpen, setGalleryOpen]
  const exclusive = useCallback(
    (setSelf: React.Dispatch<React.SetStateAction<boolean>>, onOpen?: () => void) => {
      setChatOpen(false)
      panelSetters.forEach((set) => {
        if (set !== setSelf) set(false)
      })
      setSelf((o) => {
        if (!o && onOpen) onOpen()
        return !o
      })
    },
    // panelSetters are stable useState setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    []
  )
  const send = useCallback(
    (kind: 'getSettings' | 'getProfile' | 'getNotifications' | 'getEmotes' | 'getWearables' | 'getCommunities' | 'getMap') => {
      driverRef.current?.send({ kind })
    },
    []
  )
  // Cache catalog-style fetches: only request once per session. Equip/join/setSetting
  // re-emit fresh data through the relay, so we never need to re-pull on reopen. Avoids
  // hammering the catalyst every time a menu is reopened.
  const ensure = useCallback(
    (kind: 'getSettings' | 'getProfile' | 'getEmotes' | 'getWearables' | 'getCommunities' | 'getGallery') => {
      if (fetchedRef.current.has(kind)) return
      fetchedRef.current.add(kind)
      driverRef.current?.send({ kind })
    },
    []
  )
  const closeAllPanels = useCallback(() => {
    setChatOpen(false)
    panelSetters.forEach((set) => set(false))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const toggleChat = useCallback(() => {
    panelSetters.forEach((set) => set(false))
    setChatOpen((o) => !o)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  // Opening chat (any path — sidebar, Enter, a queued mention) clears the unread badge.
  useEffect(() => {
    if (chatOpen) setChatUnread(0)
  }, [chatOpen])
  // "Mention" from a profile card opens chat and queues the @name; Chat consumes it into its draft.
  const mentionInChat = useCallback((name: string) => {
    panelSetters.forEach((set) => set(false))
    setChatOpen(true)
    setPendingMention(name)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  const consumeMention = useCallback(() => setPendingMention(null), [])

  // The full-screen main menu — mirrors App's `pageOpen`.
  const menuPageOpen =
    settingsOpen || backpackOpen || communitiesOpen || mapOpen || placesOpen || galleryOpen
  // What takes the screen from the chat, for requestFocusChat: the main menu, the emote wheel, and
  // the two modals App renders above everything (permission prompt, fatal error).
  chatCoveredRef.current =
    menuPageOpen || emotesOpen || permissionQueue.length > 0 || fatalError != null

  // Escape closes any open React panel/menu and returns to the world view. We only
  // intercept when a non-chat panel is open so ESC stays free for chat/the engine.
  const anyPanelOpen =
    menuPageOpen || friendsOpen || profileOpen || notificationsOpen || emotesOpen
  useEffect(() => {
    if (!anyPanelOpen) return
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      panelSetters.forEach((set) => set(false))
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [anyPanelOpen])
  const toggleFriends = useCallback(() => exclusive(setFriendsOpen), [exclusive])
  const toggleSettings = useCallback(() => exclusive(setSettingsOpen, () => ensure('getSettings')), [exclusive, ensure])
  const toggleProfile = useCallback(() => exclusive(setProfileOpen, () => ensure('getProfile')), [exclusive, ensure])
  const toggleNotifications = useCallback(() => exclusive(setNotificationsOpen, () => send('getNotifications')), [exclusive, send])
  const toggleEmotes = useCallback(() => exclusive(setEmotesOpen, () => ensure('getEmotes')), [exclusive, ensure])
  const toggleBackpack = useCallback(
    () =>
      exclusive(setBackpackOpen, () => {
        ensure('getWearables')
        ensure('getEmotes') // Backpack's Emotes tab reuses the emotes list.
      }),
    [exclusive, ensure]
  )
  const toggleCommunities = useCallback(() => exclusive(setCommunitiesOpen, () => ensure('getCommunities')), [exclusive, ensure])
  const toggleMap = useCallback(() => exclusive(setMapOpen, () => send('getMap')), [exclusive, send])
  // Places fetches its own HTTP data (no bridge), so opening needs no engine request.
  const togglePlaces = useCallback(() => exclusive(setPlacesOpen), [exclusive])
  const toggleGallery = useCallback(() => exclusive(setGalleryOpen, () => ensure('getGallery')), [exclusive, ensure])
  const loadGalleryPhoto = useCallback((id: string) => {
    driverRef.current?.send({ kind: 'getGalleryPhoto', id })
  }, [])
  const removeGalleryPhoto = useCallback((id: string) => {
    driverRef.current?.send({ kind: 'deleteGalleryPhoto', id })
    // Optimistically drop it; the bridge re-emits the full gallery to confirm.
    setGalleryPhotos((list) => list.filter((p) => p.id !== id))
    setGalleryStorage((s) => ({ ...s, current: Math.max(0, s.current - 1) }))
  }, [])
  const teleport = useCallback((x: number, y: number) => {
    driverRef.current?.send({ kind: 'teleport', x, y })
  }, [])
  const changeRealm = useCallback((realm: string) => {
    driverRef.current?.send({ kind: 'changeRealm', realm })
  }, [])
  const resolvePermission = useCallback(
    (id: number, allow: boolean, level: PermissionLevelChoice) => {
      setPermissionQueue((q) => {
        const req = q.find((r) => r.id === id)
        if (req) {
          driverRef.current?.send({ kind: 'permissionResolve', id, ty: req.ty, allow, level, scene: req.scene, realm: req.realm })
        }
        return q.filter((r) => r.id !== id)
      })
    },
    []
  )
  // Post-jump-in Places picker: choose a destination (or null to skip → Genesis Plaza), then leave
  // the picker. A world switches realm now; a parcel is teleported once the avatar spawns.
  const pickDestination = useCallback((dest: Destination) => {
    const driver = driverRef.current
    if (driver == null) return
    setBusy(true)
    setDestinationPicked(true) // flip to the loading overlay first

    // Boot the engine straight at the chosen destination, then run the deferred login. `engine_run`
    // is heavy and runs on the shared main thread, so defer it a paint (rAF, setTimeout fallback) so
    // the loading overlay is on screen before the freeze — same trick as the login loader. Run once.
    let ran = false
    const run = (): void => {
      // Bail if the session unmounted during the deferred kick — cleanup nulls driverRef, so this
      // guards against launching on a disposed driver (the rAF/timeout aren't otherwise cancellable).
      if (ran || driverRef.current == null) return
      ran = true
      const runDeferredLogin = (): void => {
        const login = pendingLogin.current
        pendingLogin.current = null
        Promise.resolve(login?.(driver))
          .then(() => setBusy(false))
          .catch((e: unknown) => {
            console.error('[login] post-launch login failed:', e)
            // The engine driver rejects with a RAW STRING (wasm-bindgen JsValue), not an Error —
            // e.message would be undefined and the login screen would show no error at all.
            const msg = e instanceof Error ? e.message : String(e)
            setError(msg !== '' ? msg : 'Login failed')
            setBusy(false)
            // Back to the LOGIN screen, not the picker: the picker renders no error, and the
            // login screen is where the retry / profile-reset actions live. The engine stays
            // launched (start()'s __bevyStarted guard makes the next launch a no-op).
            setSubmitted(false)
            setDestinationPicked(false)
          })
      }
      // No launch = the engine is already running at its own start realm (native): the pick maps
      // to runtime directives instead of boot parameters. A world switches realm now (works
      // pre-login); a parcel teleport needs a spawned player, so it's held until playerReady.
      // A parcel pick sends no realm change: if the picker was reachable at all the engine
      // omitted ?realm=, which it only does when it booted on the HUD's own DEFAULT_REALM —
      // a changeRealm to the same realm is NOT a no-op (full scene purge + reconnect). Skip
      // likewise keeps the engine's own start realm.
      if (driver.launch == null) {
        // Deferred to the playerReady flush only if the player hasn't spawned yet. Fresh sign-in
        // completes login BEFORE the picker, so playerReady has usually fired by pick time and the
        // flush would never run again — send immediately then. An immediate teleport still lands
        // after a just-sent changeRealm: the engine applies teleports after realm changes and
        // overrides the spawn position.
        const sendParcel = (x: number, y: number): void => {
          if (playerReadyRef.current) driver.send({ kind: 'teleport', x, y })
          else pendingParcel.current = { x, y }
        }
        if (dest?.kind === 'world') {
          driver.send({ kind: 'changeRealm', realm: dest.realm })
          const [x, y] = (dest.position ?? '').split(',').map(Number)
          if (Number.isFinite(x) && Number.isFinite(y)) sendParcel(x, y)
        } else if (dest?.kind === 'parcel') {
          sendParcel(dest.x, dest.y)
        }
        runDeferredLogin()
        return
      }
      bootPollStop.current = false
      driverRef.current?.clearEnginePanic?.() // start clean so the boot poll only sees THIS launch's panic
      // World by realm, parcel by spawn position, skip at 0,0 (Genesis). Nothing loaded before this,
      // so only the chosen scene streams in. (No-op on the mock, which has no engine to launch.)
      // Parcels pass the MAIN realm explicitly — the iframe's initialRealm may carry a ?realm
      // override (possibly an invalid world after a failed validation), and inheriting it would
      // strand a Genesis pick "Reconnecting to the realm" forever.
      try {
        if (dest == null) driver.launch?.(DEFAULT_REALM, '0,0')
        else if (dest.kind === 'world') driver.launch?.(dest.realm, dest.position)
        else driver.launch?.(DEFAULT_REALM, `${dest.x},${dest.y}`)
      } catch (e) {
        // A boot-time engine panic throws synchronously out of launch() (a generic "unreachable"
        // wasm trap). The readable message is captured on the iframe via enginePanic().
        const panic = driverRef.current?.enginePanic?.()?.message
        setFatalError({ message: panic ?? (e as Error)?.message ?? 'The engine failed to start.', source: 'launch' })
        driverRef.current?.clearEnginePanic?.()
        setBusy(false)
        return
      }
      // A boot panic can also surface a frame or two AFTER launch() returns (async wasm init / OnceCell)
      // — launch() returns normally, so poll the panic hook during boot and raise it as a FATAL 'launch'
      // error, not the dismissable 'runtime' crash the heartbeat watchdog would otherwise mislabel it as.
      // Stops on world-entry (bootPollStop) or after the window elapses.
      let polls = 0
      const pollPanic = (): void => {
        if (bootPollStop.current) return
        const panic = driverRef.current?.enginePanic?.()?.message
        if (panic != null) {
          setFatalError((prev) => prev ?? { message: panic, source: 'launch' })
          driverRef.current?.clearEnginePanic?.()
          return
        }
        if (++polls < 24) pollTimer.current = setTimeout(pollPanic, 250) // ~6s boot window
      }
      pollTimer.current = setTimeout(pollPanic, 250)
      runDeferredLogin()
    }
    requestAnimationFrame(() => requestAnimationFrame(run))
    setTimeout(run, 60)
  }, [])
  // ?position=x,y / ?realm= (parity with the plain engine page): skip the Places picker and launch
  // straight there. realm wins when both are given, carrying the position along — letting
  // ?position shadow ?realm made a reload in a custom realm respawn in Genesis at the same
  // coordinates (a parcel launch passes DEFAULT_REALM explicitly). The engine's URL sync only
  // writes ?position when the realm honours one; realms with fixed scene urns (worlds) spawn at
  // their base scene and ignore it anyway. Consumed once — after a sign-out the picker shows
  // normally.
  const urlDestination = useRef<Destination>(
    (() => {
      const q = new URLSearchParams(location.search)
      const raw = q.get('position')
      const [x, y] = raw?.split(',').map((n) => parseInt(n.trim(), 10)) ?? []
      const hasPosition = Number.isFinite(x) && Number.isFinite(y)
      const realm = q.get('realm')
      if (realm != null && realm !== '')
        return { kind: 'world', realm, position: hasPosition ? `${x},${y}` : undefined }
      if (hasPosition) return { kind: 'parcel', x, y }
      return null
    })()
  )
  const validatingRealm = useRef(false)
  useEffect(() => {
    if (!submitted || destinationPicked || urlDestination.current == null) return
    const dest = urlDestination.current
    if (dest.kind === 'world' && driverRef.current?.launch == null) {
      // Native: ?realm= is injected by the engine from its own --server, so the engine is
      // already there — skip the picker and keep the realm (the no-launch pickDestination(null)
      // path). No validation fetch either: the engine booted on this realm, and preview/file
      // realms wouldn't pass the worlds-server about probe anyway.
      urlDestination.current = null
      pickDestination(null)
      return
    }
    if (dest != null && dest.kind === 'world') {
      if (validatingRealm.current) return
      validatingRealm.current = true
      const base =
        dest.realm.endsWith('.dcl.eth') && !dest.realm.startsWith('https://')
          ? `https://worlds-content-server.decentraland.org/world/${dest.realm}`
          : dest.realm
      // Launching against an unreachable realm strands the engine in a cryptic login failure, so
      // block up front: 404 → not found, no/failed answer (incl. timeout) → unreachable.
      const timeout = new Promise<null>((resolve) => setTimeout(() => resolve(null), 4000))
      const unreachable = (): void =>
        setFatalError({ message: `The world "${dest.realm}" isn't reachable right now.`, source: 'realm' })
      Promise.race([fetch(`${base.replace(/\/+$/, '')}/about`), timeout])
        .then((r) => {
          if (r?.ok) pickDestination(dest)
          else if (r?.status === 404)
            setFatalError({ message: `The world "${dest.realm}" doesn't exist.`, source: 'realm' })
          else unreachable()
        })
        .catch(unreachable)
        .finally(() => {
          urlDestination.current = null
          validatingRealm.current = false
        })
      return
    }
    urlDestination.current = null
    pickDestination(dest)
  }, [submitted, destinationPicked, pickDestination])
  const equipWearables = useCallback((urns: string[]) => {
    driverRef.current?.send({ kind: 'equip', urns })
    // Optimistically reflect the new equipped set so the button flips to "Unequip" immediately —
    // the engine deploy + wearables re-emit lags, and a failed deploy never re-emits at all.
    setWearables((list) => list.map((w) => ({ ...w, equipped: urns.some((u) => u === w.urn || u.startsWith(`${w.urn}:`)) })))
  }, [])
  const previewWearables = useCallback((urns: string[] | null) => {
    driverRef.current?.send({ kind: 'previewAvatar', urns })
  }, [])
  const createCommunity = useCallback(
    (input: { name: string; description: string; privacy: 'public' | 'private'; discoverable: boolean }) => {
      driverRef.current?.send({ kind: 'createCommunity', ...input })
    },
    []
  )
  const joinCommunity = useCallback((id: string) => {
    driverRef.current?.send({ kind: 'joinCommunity', id })
  }, [])
  const leaveCommunity = useCallback((id: string) => {
    driverRef.current?.send({ kind: 'leaveCommunity', id })
  }, [])
  const loadCommunityDetail = useCallback((id: string) => {
    setCommunityDetail(null) // clear stale detail while the new one loads
    driverRef.current?.send({ kind: 'getCommunityDetail', id })
  }, [])
  const playEmote = useCallback((urn: string) => {
    driverRef.current?.send({ kind: 'triggerEmote', urn })
    setEmotesOpen(false)
  }, [])
  // Assign an owned emote to a wheel slot (urn:'' clears it); optimistically reflect the slot move
  // so the UI updates before the engine round-trips the new profile.
  const equipEmote = useCallback((slot: number, urn: string) => {
    driverRef.current?.send({ kind: 'equipEmote', slot, urn })
    setEmotes((list) =>
      list.map((e) => {
        if (e.urn === urn) return { ...e, slot } // the newly assigned one
        if (e.slot === slot) return { ...e, slot: undefined } // whatever was in that slot
        return e
      })
    )
  }, [])
  const toggleMic = useCallback(() => {
    setMic((m) => {
      driverRef.current?.send({ kind: 'setMic', enabled: !m.enabled })
      return { ...m, enabled: !m.enabled } // optimistic; relay confirms
    })
  }, [])
  const markNotificationsRead = useCallback(() => {
    const unreadIds = notifications.filter((n) => !n.read).map((n) => n.id)
    if (unreadIds.length === 0) return
    // Persist to the notifications service so it survives a re-fetch on reopen.
    driverRef.current?.send({ kind: 'markNotificationsRead', ids: unreadIds })
    setNotifications((prev) => prev.map((n) => (n.read ? n : { ...n, read: true })))
  }, [notifications])
  // Fetch another user's passport (View Profile). The reply arrives as a 'userProfile'
  // message and lands in the userProfiles cache.
  const requestUserProfile = useCallback((address: string) => {
    driverRef.current?.send({ kind: 'getUserProfile', address })
  }, [])
  const friendAct = useCallback((op: FriendAction, address: string) => {
    driverRef.current?.send({ kind: 'friendAction', op, address })
  }, [])
  const settingSet = useCallback((name: string, value: number) => {
    driverRef.current?.send({ kind: 'setSetting', name, value })
  }, [])

  const nav = useCallback((action: NavAction) => {
    // Opening another panel closes the React panels (single active panel).
    closeAllPanels()
    driverRef.current?.send({ kind: 'navAction', action })
  }, [closeAllPanels])

  const logout = useCallback(() => {
    driverRef.current?.logout().catch((e: Error) => console.error('[session] logout failed', e))
    clearStoredLogins() // drop the same-domain SSO identity for this origin
    setStored(null)
    setStatus('sign-in-or-guest')
    closeAllPanels()
    setPlayerReady(false)
    setSubmitted(false) // back to the login screen
    setDestinationPicked(false) // re-show the picker on the next jump-in
    pendingLogin.current = null
  }, [closeAllPanels])

  const setEngineViewport = useCallback(
    (region: 'map' | 'avatarPreview', rect: { x: number; y: number; width: number; height: number } | null) => {
      driverRef.current?.send({ kind: 'engineViewport', region, rect })
    },
    []
  )

  // Show the loader BEFORE starting login. The engine's WASM/GPU init runs heavily on the shared
  // main thread and freezes whatever's on screen; starting it while the login screen is still up
  // hangs the login UI (the frozen "Jump in" button). So flip to the loader and let it paint (two
  // frames) first, THEN kick off the engine work — the freeze then happens behind the loader, where
  // it reads as loading. On failure, fall back to the login screen.
  const submitLogin = useCallback(
    (loginCall: (driver: LoginDriver) => Promise<unknown>) => {
      const driver = driverRef.current
      if (driver == null || busy || !engineReady) return
      setError(null)
      // Don't log in yet — capture the login and show the destination picker. The engine is warm
      // (WASM + GPU) but hasn't loaded any scene; pickDestination launches it at the chosen place and
      // runs this login then. Deferring the engine work to the pick is what avoids the wasted load.
      pendingLogin.current = loginCall
      setSubmitted(true)
    },
    [busy, engineReady]
  )

  const exploreAsGuest = useCallback(() => submitLogin((d) => d.loginGuest()), [submitLogin])
  // Reuse the existing login. The driver picks the path its backend supports (console
  // `/login_identity` for the engine, `loginPrevious` over the bridge).
  const jumpIn = useCallback(() => submitLogin((d) => d.jumpIn()), [submitLogin])
  // The engine tags profile-fetch login failures (vs bad credentials etc.) with this marker —
  // mirrors PROFILE_FETCH_FAILED in system_bridge. Only then do we offer the destructive reset.
  const profileFetchFailed = error != null && error.includes('profile fetch failed')
  // Explicit recovery from an unfetchable/corrupt profile: retry, continuing with a default
  // profile if it still fails. The engine deploys that default, permanently replacing the
  // account's server-side profile — the button copy must carry that warning.
  const resetProfileAndJumpIn = useCallback(() => submitLogin((d) => d.jumpIn(true)), [submitLogin])

  // Fresh sign-in (or signing in with a different account). Web: bounce to the same-domain
  // auth site, which writes the identity back to this origin's localStorage and redirects
  // here. Native (the driver has loginNew): that redirect would resolve against cef:// and
  // 404 into the asset server — instead run the engine's remote-wallet flow, which opens the
  // auth site in the user's EXTERNAL browser and streams back a verification code to show.
  // The auth runs now (not deferred like Jump in — it needs the user at the code panel);
  // approval means the engine is already logged in, so the destination pick has no deferred
  // login left to run.
  const startWithAccount = useCallback(() => {
    if (busy) return
    const driver = driverRef.current
    if (driver?.loginNew == null) {
      redirectToAuth()
      return
    }
    const attempt = ++authAttempt.current
    setError(null)
    setBusy(true)
    setAuth({ code: null })
    driver
      .loginNew()
      .then(() => {
        if (attempt !== authAttempt.current) return // cancelled meanwhile
        setAuth(null)
        setBusy(false)
        pendingLogin.current = null
        setSubmitted(true)
      })
      .catch((e: unknown) => {
        if (attempt !== authAttempt.current) return // cancelled: the rejection is expected
        setAuth(null)
        setBusy(false)
        const msg = e instanceof Error ? e.message : String(e)
        setError(msg !== '' ? msg : 'Sign-in failed')
      })
  }, [busy])

  // Abort an in-flight fresh sign-in: drop the engine's login task and invalidate the pending
  // loginNew promise (it settles late — as cancelled from the relay or rejected — and is ignored).
  const cancelLogin = useCallback(() => {
    authAttempt.current++
    setAuth(null)
    setBusy(false)
    driverRef.current?.loginCancel().catch(() => {})
  }, [])

  // "Use a different account" shows the sign-in/guest screen (Start with account + Explore as
  // guest) rather than jumping straight to auth — matching the reference scene, and the only way a
  // returning user can reach Explore as Guest.
  const useDifferentAccount = useCallback(() => {
    if (busy) return
    setStatus('sign-in-or-guest')
  }, [busy])

  // Embedded guest auto-login (?guest=1): skip the sign-in screen entirely and
  // enter as a guest the moment the engine is warm. Used by the sites `/discover`
  // embed, which shows the scene as a guest preview with no sign-in UI. The URL
  // destination (?position / ?realm) is auto-picked by the effect above, so this
  // takes the guest straight into the scene. Fires once — `submitted` flips true
  // on the first call and gates re-entry.
  const autoGuest = useRef(new URLSearchParams(location.search).get('guest') === '1')
  useEffect(() => {
    if (!autoGuest.current || !engineReady || submitted) return
    exploreAsGuest()
  }, [engineReady, submitted, exploreAsGuest])

  // Render-settle. When the scene flips from loading → loaded (visible true→false), hold the loader
  // a beat longer while the engine actually renders the world (compiling shaders / uploading
  // textures) — otherwise it's revealed as black models. Gated on the engine's `renderBusy` probe
  // and capped by MAX_REVEAL_MS so it never hangs. Mock has no engine to wait for (no renderBusy) →
  // no delay. Applies to every scene stream, not just the first, so crossings don't flash black.
  const [revealing, setRevealing] = useState(false)
  const prevSceneVisible = useRef<boolean | undefined>(undefined)
  useEffect(() => {
    const visible = scene?.visible
    const justLoaded = prevSceneVisible.current === true && visible === false
    prevSceneVisible.current = visible
    if (!justLoaded || typeof driverRef.current?.renderBusy !== 'function') return
    setRevealing(true)
    const startedAt = performance.now()
    let timer: ReturnType<typeof setTimeout>
    const tick = (): void => {
      const elapsed = performance.now() - startedAt
      const stillRendering = driverRef.current?.renderBusy?.() === true
      if ((elapsed >= MIN_REVEAL_MS && !stillRendering) || elapsed >= MAX_REVEAL_MS) {
        setRevealing(false)
        return
      }
      timer = setTimeout(tick, 120)
    }
    timer = setTimeout(tick, MIN_REVEAL_MS)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scene?.visible])

  // Mirror the engine's native loading screen (SDK7 SceneLoadingWindow: `if (!visible) return null`).
  // The engine keeps its loader visible until the player's scene is rendered, and flips it back on
  // for each scene streamed into Genesis Plaza. We debounce the *reveal* (loading→world) so a brief
  // `visible` gap between scenes doesn't flash the HUD; the loader still appears INSTANTLY whenever
  // loading re-asserts. Loading = scene visible, or not spawned, or the render-settle still holding.
  // No state received yet (scene == null) counts as loading: the loading stream is the
  // bridge-scene's domain, and until it's running and reports otherwise the world isn't ready
  // (on native the engine relay itself sends no loading state at all).
  const loadingNow = scene?.visible !== false || !playerReady || revealing
  const [loaderActive, setLoaderActive] = useState(true)
  useEffect(() => {
    if (loadingNow) {
      setLoaderActive(true)
      return
    }
    // No engine to stream scenes (mock / tests) → nothing to bridge, reveal immediately.
    if (typeof driverRef.current?.renderBusy !== 'function') {
      setLoaderActive(false)
      return
    }
    const t = setTimeout(() => setLoaderActive(false), REVEAL_DEBOUNCE_MS)
    return () => clearTimeout(t)
  }, [loadingNow])

  const phase: SessionPhase = !submitted
    ? 'login'
    : !destinationPicked
      ? urlDestination.current != null
        ? 'entering' // a ?position/?realm launch is about to fire — never flash the picker
        : 'picking'
      : loaderActive
        ? 'entering'
        : 'world'

  return {
    phase,
    pickDestination,
    scene,
    fatalError,
    reload: () => location.reload(),
    dismissFatal: () => {
      // Re-arm the iframe watchdog (it left its `shown` flag set when it bridged the crash) so a later
      // genuine crash still surfaces, and clear the stashed panic so it isn't re-read stale.
      driverRef.current?.rearmCrashWatchdog?.()
      driverRef.current?.clearEnginePanic?.()
      setFatalError(null)
    },
    hover,
    cursorLocked,
    proximity,
    chat: {
      messages,
      send: sendChat,
      open: chatOpen,
      toggle: toggleChat,
      members,
      mention: mentionInChat,
      pendingMention,
      consumeMention,
      unread: chatUnread,
      focusTick: chatFocusTick,
      requestFocus: requestFocusChat
    },
    friends: {
      available: friendsData.available,
      list: friendsData.friends,
      received: friendsData.received,
      sent: friendsData.sent,
      blocked: friendsData.blocked,
      open: friendsOpen,
      toggle: toggleFriends,
      act: friendAct
    },
    settings: { list: settings, open: settingsOpen, toggle: toggleSettings, set: settingSet },
    profile: { data: profile, open: profileOpen, toggle: toggleProfile },
    userProfiles,
    requestUserProfile,
    notifications: {
      list: notifications,
      unread: notifications.reduce((n, x) => n + (x.read ? 0 : 1), 0),
      open: notificationsOpen,
      toggle: toggleNotifications,
      markAllRead: markNotificationsRead
    },
    emotes: { list: emotes, open: emotesOpen, toggle: toggleEmotes, play: playEmote, equip: equipEmote },
    backpack: { list: wearables, open: backpackOpen, toggle: toggleBackpack, equip: equipWearables, preview: previewWearables },
    communities: { list: communities, open: communitiesOpen, toggle: toggleCommunities, create: createCommunity, join: joinCommunity, leave: leaveCommunity, detail: communityDetail, loadDetail: loadCommunityDetail },
    map: { x: mapParcel.x, y: mapParcel.y, open: mapOpen, toggle: toggleMap, teleport, changeRealm },
    places: { open: placesOpen, toggle: togglePlaces },
    gallery: {
      list: galleryPhotos,
      current: galleryStorage.current,
      max: galleryStorage.max,
      loaded: galleryLoaded,
      open: galleryOpen,
      toggle: toggleGallery,
      metas: galleryMetas,
      loadPhoto: loadGalleryPhoto,
      remove: removeGalleryPhoto
    },
    permissions: { pending: permissionQueue, resolve: resolvePermission },
    mic: { enabled: mic.enabled, available: mic.available, toggle: toggleMic },
    nav,
    setEngineViewport,
    logout,
    menuOpen,
    login: {
      status,
      account: prevUserId ?? (stored ? rootAddress(stored.identity) : null),
      busy,
      error,
      engineReady,
      loadProgress,
      loadStep,
      startWithAccount,
      authPending: auth != null,
      authCode: auth?.code ?? null,
      cancelLogin,
      exploreAsGuest,
      jumpIn,
      profileFetchFailed,
      resetProfileAndJumpIn,
      useDifferentAccount
    }
  }
}
