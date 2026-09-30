// PlaceCard — a place from the places API on the shared DiscoverCard.

import { DiscoverCard } from '../../design'
import {
  placeCoords,
  placeCreator,
  placeIsFeatured,
  placePlayers,
  type DiscoverPlace
} from './placesApi'

export function PlaceCard({ place, onClick }: { place: DiscoverPlace; onClick: () => void }): React.JSX.Element {
  const players = placePlayers(place)
  const coords = placeCoords(place)
  const creator = placeCreator(place)
  const isWorld = place.world === true
  return (
    <DiscoverCard
      id={place.id}
      title={place.title || (isWorld ? (place.world_name ?? '') : 'Untitled')}
      image={place.image}
      live={players > 0}
      count={players}
      featured={placeIsFeatured(place)}
      creator={{ name: creator, initial: (creator || place.title || '?').trim().charAt(0).toUpperCase(), hueSeed: creator || place.id }}
      location={coords ? { text: coords, world: isWorld } : undefined}
      onClick={onClick}
    />
  )
}
