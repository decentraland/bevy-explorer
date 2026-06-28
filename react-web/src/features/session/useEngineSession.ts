// Top-level session orchestration: login → entering (scene loading) → world.
// Owns the driver and exposes the login flow + scene-loading state + phase.

import { useCallback, useEffect, useRef, useState } from 'react'
import { clearStoredLogins, getStoredLogin, redirectToAuth, rootAddress, type StoredLogin } from '../auth/sso'
import type { LoginDriver } from '../../engine/driver'
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
export type Destination = { kind: 'parcel'; x: number; y: number } | { kind: 'world'; realm: string } | null

export interface LoginFlow {
  status: LoginStatus
  /** Root wallet address of the stored SSO identity (shown on the "welcome back" screen). */
  account: string | null
  busy: boolean
  error: string | null
  /** Engine has booted and can accept the login command. The engine-driven CTAs (Jump in / Explore)
   *  stay disabled until this is true so a click never lands in a silent wait. Always true for mock. */
  engineReady: boolean
  /** Fresh sign-in → redirect to the same-domain auth site. */
  startWithAccount: () => void
  exploreAsGuest: () => void
  /** Reuse the stored SSO identity (hand it to the engine). */
  jumpIn: () => void
  /** Sign in with a different account → auth site. */
  useDifferentAccount: () => void
}

export interface EngineSession {
  phase: SessionPhase
  /** Post-jump-in Places picker: choose where to spawn (or null to skip → Genesis Plaza). */
  pickDestination: (dest: Destination) => void
  login: LoginFlow
  scene: SceneLoadingState | null
  /** World-entity hover hints under the reticle (empty = nothing hovered). */
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

  // Past login → waiting for the world.
  const [submitted, setSubmitted] = useState(false)
  // The post-jump-in Places picker: stay in 'picking' until the user chooses a destination (or skips).
  const [destinationPicked, setDestinationPicked] = useState(false)
  // Deferred login: the login call captured on Jump in, run only once the user picks a destination
  // (so the engine is launched straight at that destination instead of loading Genesis Plaza first).
  const pendingLogin = useRef<((driver: LoginDriver) => Promise<unknown>) | null>(null)
  const [playerReady, setPlayerReady] = useState(false)
  const [scene, setScene] = useState<SceneLoadingState | null>(null)
  const [hover, setHover] = useState<HoverAction[]>([])
  const [proximity, setProximity] = useState<ProximityTip[]>([])
  const [cursorLocked, setCursorLocked] = useState(false)
  const [messages, setMessages] = useState<ChatLine[]>([])
  const [members, setMembers] = useState<NearbyMember[]>([])
  const [chatOpen, setChatOpen] = useState(true)
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
          if (msg.name === 'playerReady') setPlayerReady(true)
          break
        case 'sceneLoading':
          setScene(msg.state)
          break
        case 'hover':
          setHover(msg.actions)
          break
        case 'cursorLock':
          setCursorLocked(msg.locked)
          break
        case 'proximity':
          setProximity(msg.tips)
          break
        case 'chat':
          setMessages((prev) =>
            [...prev, { ...msg.chat, id: chatId.current++, ts: Date.now() }].slice(
              -MAX_CHAT_LINES
            )
          )
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
      return
    }
    if (driver.engineReady()) {
      setEngineReady(true)
      return
    }
    const id = setInterval(() => {
      if (driverRef.current?.engineReady?.() === true) {
        setEngineReady(true)
        clearInterval(id)
      }
    }, 200)
    return () => clearInterval(id)
  }, [])

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

  const sendChat = useCallback((text: string) => {
    const trimmed = text.trim()
    if (trimmed)
      driverRef.current?.send({ kind: 'sendChat', message: trimmed, channel: 'Nearby' })
  }, [])

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

  // Escape closes any open React panel/menu and returns to the world view. We only
  // intercept when a non-chat panel is open so ESC stays free for chat/the engine.
  const anyPanelOpen =
    friendsOpen || settingsOpen || profileOpen || notificationsOpen ||
    emotesOpen || backpackOpen || communitiesOpen || mapOpen || placesOpen || galleryOpen
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
      if (ran) return
      ran = true
      // World by realm, parcel by spawn position, skip at 0,0 (Genesis). Nothing loaded before this,
      // so only the chosen scene streams in. (No-op on the mock, which has no engine to launch.)
      if (dest == null) driver.launch?.(undefined, '0,0')
      else if (dest.kind === 'world') driver.launch?.(dest.realm, undefined)
      else driver.launch?.(undefined, `${dest.x},${dest.y}`)
      const login = pendingLogin.current
      pendingLogin.current = null
      Promise.resolve(login?.(driver))
        .then(() => setBusy(false))
        .catch((e: Error) => {
          setError(e.message)
          setBusy(false)
          setDestinationPicked(false) // back to the picker on failure
        })
    }
    requestAnimationFrame(() => requestAnimationFrame(run))
    setTimeout(run, 60)
  }, [])
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

  // Fresh sign-in (or signing in with a different account): bounce to the same-domain auth
  // site, which writes the identity back to this origin's localStorage and redirects here.
  const startWithAccount = useCallback(() => {
    if (busy) return
    redirectToAuth()
  }, [busy])

  const useDifferentAccount = useCallback(() => {
    if (busy) return
    redirectToAuth()
  }, [busy])

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
  const loadingNow = scene?.visible === true || !playerReady || revealing
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
      ? 'picking'
      : loaderActive
        ? 'entering'
        : 'world'

  return {
    phase,
    pickDestination,
    scene,
    hover,
    cursorLocked,
    proximity,
    chat: { messages, send: sendChat, open: chatOpen, toggle: toggleChat, members },
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
      startWithAccount,
      exploreAsGuest,
      jumpIn,
      useDifferentAccount
    }
  }
}
