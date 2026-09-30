// React Places — a full-screen page inside MainMenuShell (mirrors CommunitiesPage): browse
// Decentraland Places / Worlds and teleport to one. The browser body (tabs, search, categories,
// grid) is shared with the post-jump-in picker via PlacesBrowser; here a card click teleports
// (parcel) or visits (world) and closes the page.

import { MainMenuShell } from '../menu/MainMenuShell'
import { PlacesBrowser } from './PlacesBrowser'
import { placeTeleport, type DiscoverPlace } from './placesApi'
import type { ProfileState } from '../session/useEngineSession'

export interface PlacesState {
  open: boolean
  toggle: () => void
}

export function PlacesPage({
  places,
  profile,
  onNavigate,
  onTeleport,
  onVisitWorld
}: {
  places: PlacesState
  profile: ProfileState
  onNavigate: (page: string) => void
  onTeleport: (x: number, y: number) => void
  onVisitWorld: (realm: string) => void
}): React.JSX.Element | null {
  if (!places.open) return null

  const visit = (place: DiscoverPlace): void => {
    const t = placeTeleport(place)
    if (!t) return
    if (t.kind === 'world') onVisitWorld(t.realm)
    else onTeleport(t.x, t.y)
    places.toggle()
  }

  const p = profile.data
  return (
    <MainMenuShell
      active="places"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={places.toggle}
    >
      <PlacesBrowser onPick={visit} />
    </MainMenuShell>
  )
}
