// Icon set for the HUD sidebar nav. Two sources:
//  • src/assets/sidebar-icons/*.png — the real Unity (unity-explorer) art, statically imported
//    (hashed, base-aware, a deleted/renamed file fails the BUILD). Drawn as a CSS mask so they
//    take `currentColor` — white by default, accent on hover/active (same recolour trick as the
//    backpack icons).
//  • PATHS — Material-style SVG glyphs for the few icons with no Unity art.
import backpackPng from '../assets/sidebar-icons/backpack.png'
import chatPng from '../assets/sidebar-icons/chat.png'
import communitiesPng from '../assets/sidebar-icons/communities.png'
import emotesPng from '../assets/sidebar-icons/emotes.png'
import friendsPng from '../assets/sidebar-icons/friends.png'
import helpPng from '../assets/sidebar-icons/help.png'
import mapPng from '../assets/sidebar-icons/map.png'
import micPng from '../assets/sidebar-icons/mic.png'
import notificationsPng from '../assets/sidebar-icons/notifications.png'
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

const MASK_ART: Partial<Record<IconName, string>> = {
  backpack: backpackPng,
  chat: chatPng,
  communities: communitiesPng,
  emotes: emotesPng,
  friends: friendsPng,
  help: helpPng,
  map: mapPng,
  mic: micPng,
  notifications: notificationsPng,
  settings: settingsPng
}

// Only the icons WITHOUT Unity png art — anything present in MASK_ART renders as a mask.
const PATHS: Partial<Record<IconName, string>> = {
  // photo_camera (Material) — body + lens ring + filled lens (nonzero winding cuts the hole).
  gallery:
    'M12 15.2c1.77 0 3.2-1.43 3.2-3.2s-1.43-3.2-3.2-3.2-3.2 1.43-3.2 3.2 1.43 3.2 3.2 3.2zM9 2 7.17 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2h-3.17L15 2H9zm3 15c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5z',
  profile:
    'M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z',
  places:
    'M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5a2.5 2.5 0 1 1 0-5 2.5 2.5 0 0 1 0 5z'
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
