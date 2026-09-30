// Featured Places carousel — a 2-row horizontally-scrolling rail of small FeaturedCards (there are
// many featured places, so it pages rather than stacking endlessly). Prev/next arrows show only when
// there's actually somewhere to scroll in that direction.

import { useCallback, useEffect, useRef, useState } from 'react'
import { FeaturedCard } from './FeaturedCard'
import type { DiscoverPlace } from './placesApi'
import styles from './PlacesPage.module.css'

function Chevron({ dir }: { dir: 'left' | 'right' }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={dir === 'left' ? 'M15 18l-6-6 6-6' : 'M9 18l6-6-6-6'} />
    </svg>
  )
}

export function FeaturedCarousel({ places, onPick }: { places: DiscoverPlace[]; onPick: (place: DiscoverPlace) => void }): React.JSX.Element {
  const track = useRef<HTMLDivElement>(null)
  const [canPrev, setCanPrev] = useState(false)
  const [canNext, setCanNext] = useState(false)

  const sync = useCallback(() => {
    const el = track.current
    if (!el) return
    setCanPrev(el.scrollLeft > 1)
    setCanNext(el.scrollLeft < el.scrollWidth - el.clientWidth - 1)
  }, [])

  useEffect(() => {
    sync()
    const el = track.current
    el?.addEventListener('scroll', sync, { passive: true })
    window.addEventListener('resize', sync)
    return () => {
      el?.removeEventListener('scroll', sync)
      window.removeEventListener('resize', sync)
    }
  }, [sync, places])

  const page = (dir: 1 | -1): void => {
    const el = track.current
    if (el) el.scrollBy({ left: dir * el.clientWidth * 0.85, behavior: 'smooth' })
  }

  return (
    <div className={styles.carousel}>
      {canPrev && (
        <button type="button" className={`${styles.carouselArrow} ${styles.carouselPrev}`} aria-label="Previous" onClick={() => page(-1)}>
          <Chevron dir="left" />
        </button>
      )}
      <div className={styles.track} ref={track}>
        {places.map((place) => (
          <FeaturedCard key={place.id} place={place} onClick={() => onPick(place)} />
        ))}
      </div>
      {canNext && (
        <button type="button" className={`${styles.carouselArrow} ${styles.carouselNext}`} aria-label="Next" onClick={() => page(1)}>
          <Chevron dir="right" />
        </button>
      )}
    </div>
  )
}
