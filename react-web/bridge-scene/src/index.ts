// System scene for the React DOM HUD (react-web).
//
// It starts the bridge, which relays the engine's SystemApi to the React host over
// BroadcastChannel. Every HUD data source lives in exactly one `domains/*` file, so it's
// obvious where each piece comes from. The ONLY thing it renders is the avatar preview
// (a TextureCamera shown through React's transparent Backpack cutout); all other UI is React.

import { ReactEcsRenderer } from '@dcl/sdk/react-ecs'
import { startBridge } from './bridge'
import { registerAvatarPreview, renderAvatarPreview } from './domains/avatarPreview'
import { registerSession } from './domains/session'
import { registerProfile } from './domains/profile'
import { registerFriends } from './domains/friends'
import { registerChat } from './domains/chat'
import { registerEmotes } from './domains/emotes'
import { registerWearables } from './domains/wearables'
import { registerNotifications } from './domains/notifications'
import { registerSettings } from './domains/settings'
import { registerCommunities } from './domains/communities'
import { registerGallery } from './domains/gallery'
import { registerWorld } from './domains/world'
import { registerPointer } from './domains/pointer'
import { registerProximity } from './domains/proximity'
import { registerSystemAction } from './domains/systemAction'
import { registerAvatarPointer } from './domains/avatarPointer'
import { registerPermissions } from './domains/permissions'
import { initNametags } from './domains/nametags'

export function main(): void {
  const _log = console.log
  console.log = (...args) => { _log('[System Scene]', ...args); }

  startBridge((ctx) => {
    registerSession(ctx)
    registerProfile(ctx)
    registerFriends(ctx)
    registerChat(ctx)
    registerEmotes(ctx)
    registerWearables(ctx)
    registerNotifications(ctx)
    registerSettings(ctx)
    registerCommunities(ctx)
    registerGallery(ctx)
    registerWorld(ctx)
    registerPointer(ctx)
    registerProximity(ctx)
    registerSystemAction(ctx)
    registerAvatarPointer(ctx)
    registerPermissions(ctx)
    registerAvatarPreview(ctx)
  })

  // The avatar preview is the scene's only full-screen UI — shown through React's Backpack cutout.
  ReactEcsRenderer.setUiRenderer(renderAvatarPreview)

  // World-space UI the DOM can't track smoothly: the billboarded nametag above each avatar's head.
  initNametags()
}
