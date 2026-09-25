// Icon set for the HUD sidebar nav. Two sources:
//  • src/assets/sidebar-icons/*.png — the real Unity (unity-explorer) art, statically imported
//    (hashed, base-aware, a deleted/renamed file fails the BUILD). Drawn as a CSS mask so they
//    take `currentColor` — white by default, accent on hover/active (same recolour trick as the
//    backpack icons).
//  • PATHS — Material-style SVG glyphs for the few icons with no Unity art.
import backpackPng from '../assets/sidebar-icons/backpack.png'
import bugPng from '../assets/sidebar-icons/bug.png'
import chatPng from '../assets/sidebar-icons/chat.png'
import communitiesPng from '../assets/sidebar-icons/communities.png'
import emotesPng from '../assets/sidebar-icons/emotes.png'
import eventsPng from '../assets/sidebar-icons/events.png'
import friendsPng from '../assets/sidebar-icons/friends.png'
import galleryPng from '../assets/sidebar-icons/gallery.png'
import helpPng from '../assets/sidebar-icons/help.png'
import mapPng from '../assets/sidebar-icons/map.png'
import marketplacePng from '../assets/sidebar-icons/marketplace.png'
import micPng from '../assets/sidebar-icons/mic.png'
import notificationsPng from '../assets/sidebar-icons/notifications.png'
import placesPng from '../assets/sidebar-icons/places.png'
import settingsPng from '../assets/sidebar-icons/settings.png'

export type IconName =
  | 'profile'
  | 'notifications'
  | 'map'
  | 'communities'
  | 'backpack'
  | 'settings'
  | 'help'
  | 'mic'
  | 'friends'
  | 'chat'
  | 'emotes'
  | 'places'
  | 'gallery'
  | 'marketplace'
  | 'bug'
  | 'events'

const MASK_ART: Partial<Record<IconName, string>> = {
  backpack: backpackPng,
  bug: bugPng,
  chat: chatPng,
  communities: communitiesPng,
  emotes: emotesPng,
  events: eventsPng,
  friends: friendsPng,
  gallery: galleryPng,
  help: helpPng,
  map: mapPng,
  marketplace: marketplacePng,
  mic: micPng,
  notifications: notificationsPng,
  places: placesPng,
  settings: settingsPng
}

// Only the icons WITHOUT Unity png art — anything present in MASK_ART renders as a mask.
const PATHS: Partial<Record<IconName, string>> = {
  profile:
    'M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z'
}

export function Icon({
  name,
  size = 22
}: {
  name: IconName
  size?: number
}): React.JSX.Element {
  const art = MASK_ART[name]
  if (art != null) {
    const url = `url(${art})`
    return (
      <span
        aria-hidden="true"
        style={{
          display: 'inline-block',
          width: size,
          height: size,
          backgroundColor: 'currentColor',
          maskImage: url,
          WebkitMaskImage: url,
          maskRepeat: 'no-repeat',
          WebkitMaskRepeat: 'no-repeat',
          maskPosition: 'center',
          WebkitMaskPosition: 'center',
          maskSize: 'contain',
          WebkitMaskSize: 'contain'
        }}
      />
    )
  }
  const d = PATHS[name]
  if (d == null) throw new Error(`Icon "${name}" has neither sidebar-icons png art nor an SVG path`)
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
      <path d={d} fill="currentColor" />
    </svg>
  )
}
