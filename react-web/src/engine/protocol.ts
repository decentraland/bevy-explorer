// Wire protocol between the React page and the super-user "bridge" scene.
//
// Transport is a same-origin BroadcastChannel (exposed to the --ui super-user
// scene only — see bevy-explorer deploy/web/sandbox_worker.js). Every message is
// wrapped in an addressed Envelope so each side ignores its own posts.
//
// Domain types mirror scene/src/bevy-api/interface.ts so the bridge scene can
// forward SystemApi results verbatim.

export const BRIDGE_CHANNEL = 'bevy-ui-bridge'

/** Mirrors SystemApi.getPreviousLogin(): userId is absent for a fresh user. */
export interface PreviousLogin {
  userId: string | null
}

/** Mirrors SystemApi.loginPrevious() result. */
export interface LoginPreviousResult {
  success: boolean
  error: string
}

/** Simple promise-returning SystemApi calls the login slice needs. */
export type RpcMethod =
  | 'getPreviousLogin'
  | 'loginPrevious'
  | 'loginGuest'
  | 'loginIdentity'
  | 'loginCancel'
  | 'logout'

// ---- page -> scene ---------------------------------------------------------

export interface RpcRequest {
  kind: 'rpc:req'
  id: string
  method: RpcMethod
}

/** Send a chat message (page → engine via the scene's BevyApi.sendChat). */
export interface SendChatRequest {
  kind: 'sendChat'
  message: string
  channel: string
}

/** Sidebar nav actions the React sidebar triggers in the scene (open a menu/popup,
 *  toggle emote wheel / mic) until those panels are themselves migrated to React. */
export type NavAction =
  | 'map'
  | 'settings'
  | 'backpack'
  | 'communities'
  | 'friends'
  | 'profile'
  | 'notifications'
  | 'emotes'
  | 'mic'

export interface NavActionRequest {
  kind: 'navAction'
  action: NavAction
}

export type PageToScene =
  | RpcRequest
  | SendChatRequest
  | NavActionRequest
  | FriendActionRequest
  | GetSettingsRequest
  | SetSettingRequest
  | GetProfileRequest
  | GetUserProfileRequest
  | GetNotificationsRequest
  | MarkNotificationsReadRequest
  | GetEmotesRequest
  | TriggerEmoteRequest
  | EquipEmoteRequest
  | SetMicRequest
  | GetWearablesRequest
  | EquipRequest
  | PreviewAvatarRequest
  | GetCommunitiesRequest
  | CreateCommunityRequest
  | JoinCommunityRequest
  | LeaveCommunityRequest
  | GetInvitableCommunitiesRequest
  | InviteToCommunityRequest
  | GetCommunityDetailRequest
  | GetMapRequest
  | TeleportRequest
  | ChangeRealmRequest
  | PermissionResolveRequest
  | EngineViewportRequest
  | GetGalleryRequest
  | GetGalleryPhotoRequest
  | DeleteGalleryPhotoRequest

// ---- scene -> page ---------------------------------------------------------

export interface RpcResponse {
  kind: 'rpc:res'
  id: string
  ok: boolean
  value?: unknown
  error?: string
}

/** Fired once the local player has spawned in-world (getPlayer() non-null). */
export interface PlayerReadyEvent {
  kind: 'event'
  name: 'playerReady'
}

/** Mirrors SystemApi SceneLoadingWindow — the scene-asset loading state. */
export interface SceneLoadingState {
  visible: boolean
  realmConnected: boolean
  title: string
  pendingAssets: number | null
}

/** Streamed scene-asset loading updates (drives the React loading screen). */
export interface SceneLoadingMessage {
  kind: 'sceneLoading'
  state: SceneLoadingState
}

/** Mirrors SystemApi ChatMessageDefinition (sender_address → sender). */
export interface ChatMessage {
  sender: string
  message: string
  channel: string
}

/** Streamed incoming chat messages (scene → page). */
export interface ChatRelayMessage {
  kind: 'chat'
  chat: ChatMessage
}

/** Chat panel visibility, driven by the scene's sidebar chat icon (hud.chatOpen). */
export interface ChatVisibilityMessage {
  kind: 'chatVisibility'
  open: boolean
}

/** A nearby player (from the scene's PlayerIdentityData set). */
export interface NearbyMember {
  address: string
  name: string
  /** Avatar face snapshot URL (when the profile has loaded). */
  picture?: string
}

/** Nearby members + count, polled by the scene (chat header "Nearby · N"). */
export interface MembersMessage {
  kind: 'members'
  members: NearbyMember[]
}

/** A full menu page (map/settings/backpack/communities) is open in the scene — the
 *  React HUD (sidebar + chat) hides while it is, since the menu has its own nav. */
export interface MenuVisibilityMessage {
  kind: 'menuVisibility'
  open: boolean
}

export type FriendStatus = 'online' | 'offline' | 'away'

/** Mirrors the scene's FriendStatusData (profilePictureUrl → picture). */
export interface Friend {
  address: string
  name: string
  picture?: string
  status: FriendStatus
}

/** Mirrors the scene's FriendRequestData. */
export interface FriendRequest {
  address: string
  name: string
  picture?: string
  message?: string
  id: string
  createdAt?: number
}

/** Friends snapshot relayed from the scene's social state (hud.friends + requests).
 *  `available` is false for guests / before the relationship snapshot is seeded. */
export interface FriendsMessage {
  kind: 'friends'
  available: boolean
  friends: Friend[]
  received: FriendRequest[]
  sent: FriendRequest[]
  /** Blocked addresses (names/avatars not resolved here). */
  blocked: string[]
}

/** Friends social action (page → scene → BevyApi.social.*). Guest-disabled. */
export type FriendAction = 'request' | 'accept' | 'reject' | 'cancel' | 'delete' | 'block' | 'unblock'

export interface FriendActionRequest {
  kind: 'friendAction'
  op: FriendAction
  address: string
}

export interface SettingVariant {
  name: string
  description: string
}

/** Mirrors the engine's ExplorerSetting (BevyApi.getSettings). A setting is a
 *  Select when it has namedVariants, otherwise a numeric Slider; a 2-variant or
 *  0..1 setting renders as a Toggle. */
export interface Setting {
  name: string
  category: string
  description: string
  minValue: number
  maxValue: number
  namedVariants: SettingVariant[]
  value: number
  default: number
  stepSize: number
}

export interface SettingsMessage {
  kind: 'settings'
  settings: Setting[]
}

/** A passport achievement badge. */
export interface Badge {
  id: string
  name: string
  /** e.g. 'bronze' | 'silver' | 'gold' — for the tier ring. */
  tier?: string
  image?: string
}

/** The about-me field grid on the passport (all optional). */
export interface ProfileInfo {
  gender?: string
  birthdate?: string
  pronouns?: string
  relationship?: string
  language?: string
  profession?: string
  employment?: string
  hobby?: string
  realName?: string
}

/** The local player's profile (passport). */
export interface Profile {
  address: string
  name: string
  picture?: string
  hasClaimedName: boolean
  isGuest: boolean
  description?: string
  links?: { title: string; url: string }[]
  // --- rich passport fields (optional; populated by the passport fetch) -----
  /** Full-body avatar snapshot (catalyst `avatar.snapshots.body`) — the passport hero image. */
  bodyImage?: string
  badges?: Badge[]
  info?: ProfileInfo
  /** Mutual-friends count shown under the name. */
  mutuals?: number
  /** Camera-reel photo URLs (Photos tab). */
  photos?: string[]
}

export interface ProfileMessage {
  kind: 'profile'
  profile: Profile | null
}

export interface GetProfileRequest {
  kind: 'getProfile'
}

/** Fetch another user's full passport by address (View Profile). */
export interface GetUserProfileRequest {
  kind: 'getUserProfile'
  address: string
}

/** A fetched user's passport (kept separate from the local `profile` message so it
 *  never clobbers the local player's profile state). */
export interface UserProfileMessage {
  kind: 'userProfile'
  address: string
  profile: Profile | null
}

/** Mirrors the engine's BaseNotification (metadata varies by type). */
export interface AppNotification {
  id: string
  type: string
  timestamp: string
  read: boolean
  metadata: Record<string, unknown>
}

export interface NotificationsMessage {
  kind: 'notifications'
  notifications: AppNotification[]
}

export interface GetNotificationsRequest {
  kind: 'getNotifications'
}

/** Persist "mark as read" for the given notification ids (signed PUT in the relay). */
export interface MarkNotificationsReadRequest {
  kind: 'markNotificationsRead'
  ids: string[]
}

/** One owned emote. `slot` is its wheel slot (0–9) when currently equipped, undefined otherwise —
 *  so the backpack shows the whole collection while the wheel still finds the 10 equipped by slot. */
export interface Emote {
  slot?: number
  urn: string
  name: string
  thumbnail?: string
  rarity?: string
  /** Owned quantity (×N badge). */
  count?: number
}

export interface EmotesMessage {
  kind: 'emotes'
  emotes: Emote[]
}

/** Assign an owned emote to a wheel slot (0–9), or clear the slot with urn:''. */
export interface EquipEmoteRequest {
  kind: 'equipEmote'
  slot: number
  urn: string
}

export interface MicMessage {
  kind: 'mic'
  enabled: boolean
  available: boolean
}

export interface SetMicRequest {
  kind: 'setMic'
  enabled: boolean
}

/** The local player's current parcel (for the map marker / centering). */
export interface MapMessage {
  kind: 'mapState'
  x: number
  y: number
}

export interface GetMapRequest {
  kind: 'getMap'
}

/** Teleport to a parcel (page → scene → teleportTo). */
export interface TeleportRequest {
  kind: 'teleport'
  x: number
  y: number
}

/** Change to a world/realm (page → scene → changeRealm). `realm` is a world name
 *  (e.g. `boedo.dcl.eth`) or realm URL. */
export interface ChangeRealmRequest {
  kind: 'changeRealm'
  realm: string
}

/** A scene's pending permission prompt relayed from the engine (e.g. it wants to move you
 *  to a new realm). Shown as the React permission dialog; resolved with permissionResolve. */
export interface PermissionRequestMessage {
  kind: 'permissionRequest'
  /** Engine-assigned request id (echoed back to resolve a "Once" decision). */
  id: number
  /** PermissionType serde name, e.g. 'ChangeRealm' — React maps it to the human prompt. */
  ty: string
  /** Scene title (e.g. 'Genesis Plaza') for the dialog text. */
  sceneName: string
  /** Scene hash — the value for a Scene-level "Always" grant. */
  scene: string
  /** Realm url the request was made under — the value for a Realm-level "Always" grant. */
  realm: string
  /** Extra context line (e.g. 'Jump to DCL Kickoff Challenge?'). */
  additional?: string
}

/** Which scope an Allow/Deny applies to. `once` = just this request; the rest persist a rule. */
export type PermissionLevelChoice = 'once' | 'scene' | 'realm' | 'global'

/** The user's decision on a permission prompt (page → scene → SystemApi). */
export interface PermissionResolveRequest {
  kind: 'permissionResolve'
  id: number
  ty: string
  allow: boolean
  level: PermissionLevelChoice
  /** Scene hash + realm carried back so the scene can target a permanent grant. */
  scene: string
  realm: string
}

/**
 * Tells the scene to render an engine-backed view (the rich map, or the avatar
 * preview) into a screen rectangle that a React page has carved out as transparent.
 * `rect` is in CSS pixels relative to the viewport; null hides the view (on close).
 */
export interface EngineViewportRequest {
  kind: 'engineViewport'
  region: 'map' | 'avatarPreview'
  rect: { x: number; y: number; width: number; height: number } | null
}

/** A community (from the scene's fetchCommunities). */
export interface Community {
  id: string
  name: string
  description: string
  thumbnail?: string
  membersCount: number
  /** 'owner' | 'moderator' | 'member' | 'none' — membership of the local user. */
  role: string
  ownerName: string
  /** 'public' | 'private' — gates the join flow (public = join, private = request). */
  privacy?: string
}

export interface CommunitiesMessage {
  kind: 'communities'
  communities: Community[]
}

export interface GetCommunitiesRequest {
  kind: 'getCommunities'
}

/** Create a community (page → scene → signed multipart POST to the social-api). */
export interface CreateCommunityRequest {
  kind: 'createCommunity'
  name: string
  description: string
  privacy: 'public' | 'private'
  /** Discoverable in the directory → visibility 'all', otherwise 'unlisted'. */
  discoverable: boolean
}

export interface JoinCommunityRequest {
  kind: 'joinCommunity'
  id: string
}

export interface LeaveCommunityRequest {
  kind: 'leaveCommunity'
  id: string
}

/** Communities the local user can invite `address` to (server filters: caller is owner/mod, target not a member). */
export interface GetInvitableCommunitiesRequest {
  kind: 'getInvitableCommunities'
  address: string
}

/** Invite `address` to a community (page → scene → signed POST to the social-api). */
export interface InviteToCommunityRequest {
  kind: 'inviteToCommunity'
  communityId: string
  address: string
}

/** A community the local user can invite someone to (id + name only, per the social-api). */
export interface InvitableCommunity {
  id: string
  name: string
}

/** The invitable-communities list for `address` (empty → hide "Invite to Community"). */
export interface InvitableCommunitiesMessage {
  kind: 'invitableCommunities'
  address: string
  communities: InvitableCommunity[]
}

/** A member of a community (Members tab). */
export interface CommunityMember {
  address: string
  name: string
  /** 'owner' | 'moderator' | 'member' */
  role: string
  picture?: string
  hasClaimedName?: boolean
  /** the local user already follows them (hides "Add Friend"). */
  isFriend?: boolean
}

/** A place shared by a community (Places tab). */
export interface CommunityPlace {
  id: string
  title: string
  thumbnail?: string
  /** e.g. "-66,56" */
  positions?: string
  /** 0..1 like rate. */
  likeRate?: number
}

/** An announcement post (Announcements tab). */
export interface CommunityPost {
  id: string
  author: string
  authorAddress: string
  authorPicture?: string
  text: string
  timestamp: number
  likes: number
}

/** An upcoming event (right-hand sidebar). */
export interface CommunityEvent {
  id: string
  name: string
  thumbnail?: string
  startsAt: number
}

/** A camera-reel photo shared in the community (Photos tab). */
export interface CommunityPhoto {
  id: string
  url: string
  thumbnail?: string
}

/** Request the per-community detail when the modal opens. */
export interface GetCommunityDetailRequest {
  kind: 'getCommunityDetail'
  id: string
}
export interface CommunityDetailMessage {
  kind: 'communityDetail'
  id: string
  members: CommunityMember[]
  posts: CommunityPost[]
  places: CommunityPlace[]
  events: CommunityEvent[]
  photos: CommunityPhoto[]
}

// ---- gallery (camera reel) -------------------------------------------------

/** One camera-reel photo (compact list item). `dateTime` is the service's raw string
 *  (a unix timestamp in seconds or ms, or ISO) — parsed for month-grouping on the page. */
export interface GalleryPhoto {
  id: string
  url: string
  thumbnailUrl?: string
  dateTime: string
  /** Whether the photo is publicly shareable (only the owner can flip it). */
  isPublic?: boolean
}

/** A person captured in a photo (the detail-view "people in this photo" list). */
export interface GalleryPerson {
  address: string
  name: string
  isGuest?: boolean
}

/** Full metadata for one photo (fetched lazily when its detail view opens). */
export interface GalleryPhotoMeta {
  /** Who took the photo. */
  userName?: string
  userAddress?: string
  /** Scene name + parcel where it was taken (for "Jump In"). */
  sceneName?: string
  x?: number
  y?: number
  realm?: string
  people?: GalleryPerson[]
}

export interface GetGalleryRequest {
  kind: 'getGallery'
}

/** The local player's camera-reel photos + storage usage (current/max). */
export interface GalleryMessage {
  kind: 'gallery'
  photos: GalleryPhoto[]
  current: number
  max: number
}

/** Fetch one photo's full metadata (place + people) for its detail view. */
export interface GetGalleryPhotoRequest {
  kind: 'getGalleryPhoto'
  id: string
}

export interface GalleryPhotoMessage {
  kind: 'galleryPhoto'
  id: string
  meta: GalleryPhotoMeta | null
}

/** Delete one of the local player's photos (signed DELETE; re-emits the gallery). */
export interface DeleteGalleryPhotoRequest {
  kind: 'deleteGalleryPhoto'
  id: string
}

/** An owned wearable (backpack catalog item). */
export interface Wearable {
  urn: string
  name: string
  rarity: string
  category: string
  thumbnail?: string
  count?: number
  equipped: boolean
}

export interface WearablesMessage {
  kind: 'wearables'
  wearables: Wearable[]
}

export interface GetWearablesRequest {
  kind: 'getWearables'
}

/** Equip a new full wearable set (page → scene → BevyApi.setAvatar). */
export interface EquipRequest {
  kind: 'equip'
  urns: string[]
}

/** Preview a wearable set on the Backpack avatar WITHOUT persisting it to the profile
 *  (selecting an item, not equipping). `urns: null` clears the preview (revert to profile). */
export interface PreviewAvatarRequest {
  kind: 'previewAvatar'
  urns: string[] | null
}

export interface GetEmotesRequest {
  kind: 'getEmotes'
}

export interface TriggerEmoteRequest {
  kind: 'triggerEmote'
  urn: string
}

export interface GetSettingsRequest {
  kind: 'getSettings'
}

export interface SetSettingRequest {
  kind: 'setSetting'
  name: string
  value: number
}

/** One hover hint for a world entity under the reticle (from the engine getHoverStream). */
export interface HoverAction {
  /** The `InputAction` enum the action is bound to; React maps it to a key label (E / 🖱 / 1…). */
  button: number
  text: string
  /** false → out of range ("Too far, get closer"). */
  enabled: boolean
}
/** Hover hints to show at the pointer. Empty array = nothing hovered. */
export interface HoverMessage {
  kind: 'hover'
  actions: HoverAction[]
  /** Cursor screen-pixel position (from PrimaryPointerInfo) so the hint sits at the pointer, not the
   *  reticle. It's the screen centre while pointer-locked; absent if unavailable. */
  x?: number
  y?: number
}

/** Streamed cursor position while a hover is active (free cursor) so the tooltip follows the mouse —
 *  the hover stream only fires on enter/exit, so position updates come through here per-frame. */
export interface HoverPosMessage {
  kind: 'hoverPos'
  x: number
  y: number
}

/** Whether the engine has grabbed the mouse for camera-look (OS cursor hidden) → draw the
 *  center crosshair. Derived from PrimaryPointerInfo.screenCoordinates being absent. */
export interface CursorLockMessage {
  kind: 'cursorLock'
  locked: boolean
}

/** A proximity tooltip for an in-range world entity, anchored at its projected screen position
 *  (the bridge does the world→screen projection each frame). */
export interface ProximityTip {
  id: number
  /** Screen pixel coords of the entity (tooltip is centered on this). */
  x: number
  y: number
  actions: HoverAction[]
}
/** All in-range entity tooltips this frame (empty = none). */
export interface ProximityMessage {
  kind: 'proximity'
  tips: ProximityTip[]
}

/** A nearby avatar was clicked in the world → open their profile card, anchored at the click. */
export interface AvatarClickMessage {
  kind: 'avatarClick'
  address: string
  name: string
  /** Cursor screen-pixel position at the click (from PrimaryPointerInfo) — where to anchor the card. */
  x: number
  y: number
}

export type SceneToPage =
  | RpcResponse
  | HoverMessage
  | HoverPosMessage
  | CursorLockMessage
  | ProximityMessage
  | AvatarClickMessage
  | PlayerReadyEvent
  | SceneLoadingMessage
  | ChatRelayMessage
  | ChatVisibilityMessage
  | MembersMessage
  | MenuVisibilityMessage
  | FriendsMessage
  | SettingsMessage
  | ProfileMessage
  | UserProfileMessage
  | NotificationsMessage
  | EmotesMessage
  | MicMessage
  | WearablesMessage
  | CommunitiesMessage
  | CommunityDetailMessage
  | InvitableCommunitiesMessage
  | MapMessage
  | GalleryMessage
  | GalleryPhotoMessage
  | PermissionRequestMessage

// ---- envelope --------------------------------------------------------------

export type Envelope =
  | { to: 'scene'; msg: PageToScene }
  | { to: 'page'; msg: SceneToPage }
