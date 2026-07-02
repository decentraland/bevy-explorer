// FeaturedCard — the small HORIZONTAL card used in the Featured Places carousel, matched to
// decentraland-ui2's EventSmallCard (thumbnail left ~42%, text right: 2-line title + creator row).
// On hover the creator row is replaced by a JUMP IN button; white-glow lift like the big card.

import { useState } from 'react'
import { placeCoords, placeCreator, type DiscoverPlace } from './placesApi'
import styles from './FeaturedCard.module.css'

function hueOf(id: string): number {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360
  return h
}

function PinGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <path d="M12 21s7-6.2 7-11a7 7 0 1 0-14 0c0 4.8 7 11 7 11Z" />
      <circle cx="12" cy="10" r="2.4" />
    </svg>
  )
}
function GlobeGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3c2.5 2.6 2.5 15.4 0 18M12 3c-2.5 2.6-2.5 15.4 0 18" />
    </svg>
  )
}
function JumpInGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M5 12l9-9 4 4-9 9-5 1z" />
      <path d="M13 4l3 3M5 12l-1 7 7-1" />
    </svg>
  )
}

export function FeaturedCard({ place, onClick }: { place: DiscoverPlace; onClick: () => void }): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const coords = placeCoords(place)
  const creator = placeCreator(place)
  const isWorld = place.world === true
  const showImg = !!place.image && !failed
  const initial = (creator || place.title || '?').trim().charAt(0).toUpperCase()

  return (
    <article className={styles.card} onClick={onClick} role="button" tabIndex={0}>
      <div className={styles.thumb} style={{ ['--hue' as string]: hueOf(place.id) }}>
        {showImg && <img className={styles.thumbImg} src={place.image} alt="" draggable={false} onError={() => setFailed(true)} />}
      </div>
      <div className={styles.body}>
        <span className={styles.title} title={place.title}>{place.title || (isWorld ? place.world_name : 'Untitled')}</span>
        <div className={styles.footer}>
          {creator && (
            <span className={styles.creator}>
              <span className={styles.avatar} style={{ ['--hue' as string]: hueOf(creator || place.id) }} aria-hidden="true">{initial}</span>
              <span className={styles.by}>By <span className={styles.name} title={creator}>{creator}</span></span>
            </span>
          )}
          {coords && (
            <span className={`${styles.loc} ${isWorld ? styles.locWorld : ''}`.trim()} title={coords}>
              {isWorld ? <GlobeGlyph /> : <PinGlyph />}
              {coords}
            </span>
          )}
        </div>
        <span className={styles.jumpIn}>
          <JumpInGlyph /> Jump in
        </span>
      </div>
    </article>
  )
}
