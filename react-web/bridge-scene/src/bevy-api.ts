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

// World-entity hover events. targetType: 0=WORLD, 1=UI, 2=AVATAR. eventType: PointerEventType.
export type HoverEntry = {
  eventType: number
  enabled?: boolean
  eventInfo?: { button?: number; hoverText?: string; showFeedback?: boolean }
}
export type SystemHoverEvent = { entered: boolean; targetType: number; actions: HoverEntry[] }

// Proximity (in-range) events for world entities. entityPosition is world-space; the bridge
// projects it to screen each frame so React can anchor a tooltip on it.
export type Vec3 = { x: number; y: number; z: number }
export type SystemProximityEvent = { entered: boolean; entity: number; entityPosition: Vec3; actions: HoverEntry[] }

export type KernelFetchRequest = {
  url: string
  init: { headers?: Record<string, string>; method: 'GET' | 'POST' | 'PUT' | 'DELETE'; body?: string }
  meta: string
}
export type KernelFetchResponse = { ok: boolean; status: number; statusText?: string; body: string }

export type BevyApiInterface = {
  getSettings: () => Promise<Setting[]>
  setSetting: (name: string, value: number) => Promise<void>
  sendChat: (message: string, channel: string) => void
  getChatStream: () => Promise<AsyncIterable<ChatStreamMessage>>
  getSceneLoadingUIStream: () => Promise<AsyncIterable<SceneLoadingState>>
  getHoverStream: () => Promise<AsyncIterable<SystemHoverEvent>>
  getProximityStream: () => Promise<AsyncIterable<SystemProximityEvent>>
  getMicState: () => Promise<MicState>
  setMicEnabled: (enabled: boolean) => void
  setAvatar: (data: { equip: { wearableUrns: string[]; emoteUrns: string[]; forceRender: string[] } }) => Promise<unknown>
  kernelFetch: (req: KernelFetchRequest) => Promise<KernelFetchResponse>
  getRealmProvider: () => Promise<string>
  getPreviousLogin: () => Promise<{ userId: string | null }>
  loginPrevious: () => Promise<{ success: boolean; error: string }>
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
