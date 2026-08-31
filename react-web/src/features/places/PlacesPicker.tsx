// PlacesPicker — shown right after Jump In (phase 'picking') so the user chooses where to spawn
// instead of dropping straight into Genesis Plaza. Reuses the exact in-world Places browser; a card
// click resolves a destination (parcel teleport / world realm) and a Skip button takes the default.

import { Button } from '../../design'
import { PlacesBrowser } from './PlacesBrowser'
import { placeTeleport, type DiscoverPlace } from './placesApi'
import type { Destination } from '../session/useEngineSession'
import styles from './PlacesPicker.module.css'

export function PlacesPicker({ onPick }: { onPick: (dest: Destination) => void }): React.JSX.Element {
  return (
    <div className={styles.root}>
      <div className={styles.scroll}>
        <div className={styles.inner}>
          <div className={styles.header}>
            <div>
              <p className={styles.kicker}>Welcome back</p>
              <h1 className={styles.title}>Where do you want to go?</h1>
            </div>
            <Button variant="secondary" size="md" className={styles.skip} onClick={() => onPick(null)}>
              Skip to Genesis Plaza
            </Button>
          </div>
          <PlacesBrowser onPick={(place: DiscoverPlace) => onPick(placeTeleport(place))} />
        </div>
      </div>
    </div>
  )
}
