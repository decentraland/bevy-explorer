// Typed accessor for the engine's `~system/BevyExplorerApi` (the SystemApi), available only
// inside the super-user (--ui) scene sandbox. This is the ENGINE-side surface (raw shapes);
// the wire shapes React sees live in the shared protocol. Only the methods the domains use
// are declared — extend as needed.
import type { Setting } from '../../src/engine/protocol'

// --- raw social-service shapes (BevyApi.social.*) ---
export type FriendStatusData = {
  address: string
  name: string
  hasClaimedName: boolean
  profilePictureUrl: string
  status: 'online' | 'offline' | 'away'
}
export type FriendRequestData = {
  address: string
  name: string
  hasClaimedName: boolean
  profilePictureUrl: string
  createdAt: number
  message?: string
  id: string
}
export type BlockingStatus = { blockedUsers: string[]; blockedByUsers: string[] }

export type SocialApi = {
  getSocialInitialized: () => Promise<boolean>
  getOnlineFriends: () => Promise<FriendStatusData[]>
  getReceivedFriendRequests: () => Promise<FriendRequestData[]>
  getSentFriendRequests: () => Promise<FriendRequestData[]>
  getBlockingStatus?: () => Promise<BlockingStatus>
  getBlockedUsers: () => Promise<Array<{ address: string }>>
  sendFriendRequest: (address: string, message?: string) => Promise<unknown>
  acceptFriendRequest: (address: string) => Promise<void>
  rejectFriendRequest: (address: string) => Promise<void>
  cancelFriendRequest: (address: string) => Promise<void>
  deleteFriend: (address: string) => Promise<void>
  blockUser: (address: string) => Promise<void>
  unblockUser: (address: string) => Promise<void>
}

export type ChatStreamMessage = { sender_address: string; message: string; channel: string }
export type SceneLoadingState = { visible?: boolean; realmConnected?: boolean; title?: string; pendingAssets?: number | null }
export type MicState = { enabled: boolean; available: boolean }

// Per-player modifiers from AvatarModifierArea (privacy zones etc), only for players carrying one —
// e.g. hideProfile means the local player is standing in a DISABLE_PASSPORTS area, so `userId`'s
// passport should not be opened. Only present in the array when at least one flag is set.
export type AvatarModifierState = { userId: string; hideAvatar: boolean; hideProfile: boolean }

// World-entity hover events. targetType: 0=WORLD, 1=UI, 2=AVATAR. eventType: PointerEventType.
export type HoverEntry = {
  eventType: number
  enabled?: boolean
  // maxPlayerDistance absent/null → the entry is range-gated by camera distance only (the PBPointerEvents
  // default when neither is set is a 10m camera check), per pointer_events.proto's distance-rule doc.
  eventInfo?: { button?: number; hoverText?: string; showFeedback?: boolean; maxPlayerDistance?: number | null }
}
export type SystemHoverEvent = { entered: boolean; targetType: number; actions: HoverEntry[] }

// Proximity (in-range) events for world entities. entityPosition is world-space; the bridge
// projects it to screen each frame so React can anchor a tooltip on it.
export type Vec3 = { x: number; y: number; z: number }
export type SystemProximityEvent = { entered: boolean; entity: number; entityPosition: Vec3; actions: HoverEntry[] }

// Global engine input actions. `action` is the SystemAction variant name (e.g. 'Cancel' = Escape,
// 'Map', 'Chat'); `pressed` is press vs release. Emitted authoritatively by the engine even while it
// holds keyboard focus, so the HUD can react to Escape without a DOM keydown.
export type SystemActionEvent = { action: string; pressed: boolean }

export type KernelFetchRequest = {
  url: string
  init: { headers?: Record<string, string>; method: 'GET' | 'POST' | 'PUT' | 'DELETE'; body?: string }
  meta: string
}
export type KernelFetchResponse = { ok: boolean; status: number; statusText?: string; body: string }

// A scene's pending permission request (e.g. ChangeRealm). `ty` is the serde enum name
// (e.g. 'ChangeRealm'); `scene` is the scene HASH — resolve its title via liveSceneInfo().
export type PermissionRequestRaw = {
  ty: string
  additional?: string | null
  scene: string
  id: number
}
// Persist a permission at a level. `value` is the scene hash (Scene) or realm url (Realm),
// unused for Global. `allow: null` clears the stored value.
export type SetPermanentPermissionBody = {
  level: 'Scene' | 'Realm' | 'Global'
  value?: string
  ty: string
  allow: 'Allow' | 'Deny' | null
}

export type BevyApiInterface = {
  getSettings: () => Promise<Setting[]>
  setSetting: (name: string, value: number) => Promise<void>
  sendChat: (message: string, channel: string) => void
  getChatStream: () => Promise<AsyncIterable<ChatStreamMessage>>
  getSystemActionStream: () => Promise<AsyncIterable<SystemActionEvent>>
  getSceneLoadingUIStream: () => Promise<AsyncIterable<SceneLoadingState>>
  getHoverStream: () => Promise<AsyncIterable<SystemHoverEvent>>
  getProximityStream: () => Promise<AsyncIterable<SystemProximityEvent>>
  getMicState: () => Promise<MicState>
  setMicEnabled: (enabled: boolean) => void
  getAvatarModifiers: () => Promise<AvatarModifierState[]>
  getPermissionRequestStream: () => Promise<AsyncIterable<PermissionRequestRaw>>
  setSinglePermission: (body: { id: number; allow: boolean }) => void
  setPermanentPermission: (body: SetPermanentPermissionBody) => void
  /** Live scenes (hash → title), for resolving a permission request's scene name. */
  liveSceneInfo: () => Promise<Array<{ hash: string; title: string }>>
  setAvatar: (data: { equip: { wearableUrns: string[]; emoteUrns: string[]; forceRender: string[] } }) => Promise<unknown>
  kernelFetch: (req: KernelFetchRequest) => Promise<KernelFetchResponse>
  getRealmProvider: () => Promise<string>
  getPreviousLogin: () => Promise<{ userId: string | null }>
  loginPrevious: () => Promise<{ success: boolean; error: string }>
  /** Remote-wallet fresh sign-in: the engine opens the auth site in the external browser.
   *  `code` resolves with the verification code to display (null = none issued); `success`
   *  resolves on approval and rejects on failure/cancel. */
  loginNew: () => { code: Promise<string | null>; success: Promise<void> }
  loginGuest: () => void
  loginCancel: () => void
  logout: () => void
  social: SocialApi
}

const globalRequire = (globalThis as { require?: (module: string) => unknown }).require

// Fallback when there is no engine API (e.g. outside the super-user sandbox): calls reject
// rather than silently no-op. Typed via an identifier so it isn't an object-literal assertion.
const NO_API: Partial<BevyApiInterface> = {}

function load(): BevyApiInterface {
  try {
    if (globalRequire != null) return globalRequire('~system/BevyExplorerApi') as BevyApiInterface
  } catch (e) {
    console.error('[bevy-api] BevyExplorerApi not found', e)
  }
  return NO_API as BevyApiInterface
}

export const BevyApi: BevyApiInterface = load()
