// PlaceCard — pixel-matched to decentraland-ui2's LiveNowCard (the production "What's On" card, in
// ../../sites): a cover image with LIVE + people badges, a translucent body with the title and a
// creator row, and a full-width JUMP IN button that slides up on hover (the creator row shifts up to
// make room). Adds the Figma's top-right Featured tag and a location pill (📍coords / 🌐world, the
// latter purple so Worlds read distinctly). Image loads via a direct <img> (credentialless COEP),
// falling back to a per-id hue gradient.

import { useState } from 'react'
import { People, Pin } from '../../design'
import {
  placeCoords,
  placeCreator,
  placeIsFeatured,
  placePlayers,
  type DiscoverPlace
} from './placesApi'
import styles from './PlaceCard.module.css'

function hueOf(id: string): number {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360
  return h
}

function MedalGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="9" r="5" />
      <path d="M9 13.4 7.5 21l4.5-2.6L16.5 21 15 13.4" />
    </svg>
  )
}

function GlobeGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3c2.5 2.6 2.5 15.4 0 18M12 3c-2.5 2.6-2.5 15.4 0 18" />
    </svg>
  )
}

function JumpInGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M5 12l9-9 4 4-9 9-5 1z" />
      <path d="M13 4l3 3M5 12l-1 7 7-1" />
    </svg>
  )
}

export function PlaceCard({ place, onClick }: { place: DiscoverPlace; onClick: () => void }): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const players = placePlayers(place)
  const live = players > 0
  const featured = placeIsFeatured(place)
  const coords = placeCoords(place)
  const creator = placeCreator(place)
  const isWorld = place.world === true
  const showImg = !!place.image && !failed
  const initial = (creator || place.title || '?').trim().charAt(0).toUpperCase()

  return (
    <article className={styles.card} onClick={onClick} role="button" tabIndex={0}>
      <div className={styles.media} style={{ ['--hue' as string]: hueOf(place.id) }}>
        {showImg && <img className={styles.mediaImg} src={place.image} alt="" draggable={false} onError={() => setFailed(true)} />}

        <div className={styles.badges}>
          <div className={styles.badgeGroup}>
            {live && (
              <span className={`${styles.badge} ${styles.live}`}>
                <span className={styles.liveDot} /> LIVE
              </span>
            )}
            {players > 0 && (
              <span className={styles.badge}>
                <span className={styles.userDot} />
                <People size={13} />
                {players}
              </span>
            )}
          </div>
          {featured && (
            <span className={styles.featured}>
              <MedalGlyph /> Featured
            </span>
          )}
        </div>
      </div>

      <div className={styles.body}>
        <div className={styles.info}>
          <span className={styles.title} title={place.title}>{place.title || (isWorld ? place.world_name : 'Untitled')}</span>
          <div className={styles.creatorRow}>
            <span className={styles.avatar} style={{ ['--hue' as string]: hueOf(creator || place.id) }} aria-hidden="true">{initial}</span>
            {creator && (
              <span className={styles.by}>By <span className={styles.name} title={creator}>{creator}</span></span>
            )}
            {coords && (
              <span className={`${styles.loc} ${isWorld ? styles.locWorld : ''}`.trim()} title={coords}>
                {isWorld ? <GlobeGlyph /> : <Pin size={12} />}
                {coords}
              </span>
            )}
          </div>
        </div>

        <div className={styles.jumpInWrap}>
          <span className={styles.jumpIn}>
            <span>Jump in</span>
            <JumpInGlyph />
          </span>
        </div>
      </div>
    </article>
  )
}
