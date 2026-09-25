// DiscoverCard — the discovery card shared by Places and Events, pixel-matched to decentraland-ui2's
// LiveNowCard (the production "What's On" card, in the sites repo): a cover image with LIVE + people
// badges, a translucent body with the title and a creator row, and a full-width JUMP IN button that
// slides up on hover (the creator row shifts up to make room). Adds the Figma's top-right Featured
// tag and a location pill (📍coords / 🌐world, the latter purple so Worlds read distinctly). Image
// loads via a direct <img> (credentialless COEP), falling back to a per-id hue gradient.

import { useState } from 'react'
import { People, Pin } from './Glyphs'
import styles from './DiscoverCard.module.css'

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

/** The creator row of a place card: an initial avatar and "By <name>". */
export interface CardCreator {
  name: string
  initial: string
  hueSeed: string
}

/** The discovery card shared by Places and Events: cover, LIVE / count / Featured badges, title,
 *  a creator or a plain byline, a location pill, and the JUMP IN button. */
export function DiscoverCard({
  id,
  title,
  image,
  live = false,
  count = 0,
  featured = false,
  creator,
  byline,
  location,
  label,
  onClick
}: {
  /** Seeds the fallback cover gradient. */
  id: string
  title: string
  image?: string | null
  live?: boolean
  count?: number
  featured?: boolean
  creator?: CardCreator
  byline?: string
  location?: { text: string; world: boolean }
  label?: string
  onClick: () => void
}): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const showImg = !!image && !failed

  return (
    <article
      className={styles.card}
      onClick={onClick}
      onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && onClick()}
      role="button"
      tabIndex={0}
      aria-label={label}
    >
      <div className={styles.media} style={{ ['--hue' as string]: hueOf(id) }}>
        {showImg && <img className={styles.mediaImg} src={image} alt="" draggable={false} onError={() => setFailed(true)} />}

        <div className={styles.badges}>
          <div className={styles.badgeGroup}>
            {live && (
              <span className={`${styles.badge} ${styles.live}`}>
                <span className={styles.liveDot} /> LIVE
              </span>
            )}
            {count > 0 && (
              <span className={styles.badge}>
                <span className={styles.userDot} />
                <People size={13} />
                {count}
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
          <span className={styles.title} title={title}>{title}</span>
          <div className={styles.creatorRow}>
            {creator && (
              <>
                <span className={styles.avatar} style={{ ['--hue' as string]: hueOf(creator.hueSeed) }} aria-hidden="true">{creator.initial}</span>
                {creator.name && (
                  <span className={styles.by}>By <span className={styles.name} title={creator.name}>{creator.name}</span></span>
                )}
              </>
            )}
            {byline && <span className={styles.by}>{byline}</span>}
            {location && (
              <span className={`${styles.loc} ${location.world ? styles.locWorld : ''}`.trim()} title={location.text}>
                {location.world ? <GlobeGlyph /> : <Pin size={12} />}
                {location.text}
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
