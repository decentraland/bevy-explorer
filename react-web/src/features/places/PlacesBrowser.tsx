// The Places browser, matched to the "What's On" design: three stacked sections —
//   1. LIVE NOW    — the 4 most-populated places (a single full-width row)
//   2. FEATURED    — places tagged as points of interest
//   3. ALL          — a toolbar (Explore all / Favourites / My places + search + Any Category +
//                     Sort by) over a purple grid panel of every place.
// Live Now + Featured are curated highlights shown only on the default Explore-all view (no search,
// no category filter); searching/filtering collapses to the All grid. Shared by the in-world
// PlacesPage and the post-jump-in PlacesPicker; the host supplies an `onPick(place)` action.

import { useEffect, useMemo, useRef, useState } from 'react'
import { Dropdown, EmptyState, Heart, Pin, SearchField, Spinner } from '../../design'
import { PlaceCard } from './PlaceCard'
import { FeaturedCarousel } from './FeaturedCarousel'
import { usePlaces, type PlacesSection, type PlacesSort } from './usePlaces'
import { placeIsFeatured, placePlayers, type DiscoverPlace } from './placesApi'
import styles from './PlacesPage.module.css'

const SECTIONS: { id: PlacesSection; label: string; icon: React.ReactNode }[] = [
  { id: 'all', label: 'Explore all', icon: <CompassGlyph /> },
  { id: 'favourites', label: 'Favourites', icon: <Heart size={15} /> },
  { id: 'my', label: 'My places', icon: <Pin size={15} /> }
]

const LIVE_NOW_LIMIT = 4

// "Any Category" dropdown — labels map to the usePlaces category id (featured = only_pois).
const CATEGORY_OPTIONS = ['Any Category', 'Featured', 'Social', 'Music', 'Art', 'Game', 'Fashion', 'Education', 'Shop', 'Sports', 'Business']
function categoryId(label: string): string {
  return label === 'Any Category' ? 'all' : label.toLowerCase()
}
function categoryLabel(id: string): string {
  if (id === 'all') return 'Any Category'
  return id.charAt(0).toUpperCase() + id.slice(1)
}

const SORT_OPTIONS: { label: string; value: PlacesSort }[] = [
  { label: 'Most active', value: 'most_active' },
  { label: 'Most liked', value: 'like_score_best' },
  { label: 'Newest', value: 'created_at' },
  { label: 'Name', value: 'name' }
]

const SEARCH_DEBOUNCE_MS = 300

function CompassGlyph(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M15.5 8.5l-2 5-5 2 2-5z" />
    </svg>
  )
}

export function PlacesBrowser({
  headExtra,
  onPick
}: {
  /** Slot rendered at the far right of the toolbar (e.g. the picker's "Skip" button). */
  headExtra?: React.ReactNode
  onPick: (place: DiscoverPlace) => void
}): React.JSX.Element {
  const { places: list, loading, error, search, setSearch, category, setCategory, sort, setSort, section, setSection } = usePlaces()
  // Local input value, debounced into the hook's `search` so we don't refetch on every keystroke.
  const [draft, setDraft] = useState(search)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => setSearch(draft), SEARCH_DEBOUNCE_MS)
    return () => {
      if (timer.current) clearTimeout(timer.current)
    }
  }, [draft, setSearch])

  // Highlights are derived from the full Explore-all list (most populated first / points of interest).
  // Each place belongs to ONE section only — priority Live Now → Featured → the discover grid — so a
  // place like Genesis Plaza (populated AND a POI) doesn't repeat across all three.
  const liveNow = useMemo(
    () => [...list].filter((p) => placePlayers(p) > 0).sort((a, b) => placePlayers(b) - placePlayers(a)).slice(0, LIVE_NOW_LIMIT),
    [list]
  )
  const liveIds = useMemo(() => new Set(liveNow.map((p) => p.id)), [liveNow])
  const featured = useMemo(() => list.filter((p) => placeIsFeatured(p) && !liveIds.has(p.id)), [list, liveIds])
  const showHighlights = section === 'all' && category === 'all' && search.trim() === '' && !loading && !error
  // The discover grid shows everything not already surfaced in a highlight section above it.
  const mainList = useMemo(() => {
    if (!showHighlights) return list
    const featuredIds = new Set(featured.map((p) => p.id))
    return list.filter((p) => !liveIds.has(p.id) && !featuredIds.has(p.id))
  }, [list, showHighlights, liveIds, featured])

  const renderGrid = (items: DiscoverPlace[]): React.JSX.Element => (
    <div className={styles.grid}>
      {items.map((place) => (
        <PlaceCard key={place.id} place={place} onClick={() => onPick(place)} />
      ))}
    </div>
  )

  return (
    <>
      {showHighlights && liveNow.length > 0 && (
        <section className={styles.section}>
          <h2 className={styles.sectionTitle}>
            <span className={styles.liveDot} /> Live Now
          </h2>
          {renderGrid(liveNow)}
        </section>
      )}

      {showHighlights && featured.length > 0 && (
        <section className={styles.section}>
          <h2 className={styles.sectionTitle}>Featured Places</h2>
          <FeaturedCarousel places={featured} onPick={onPick} />
        </section>
      )}

      <div className={styles.toolbar}>
        <div className={styles.tabs} role="tablist" aria-label="Places sections">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              role="tab"
              aria-selected={s.id === section}
              className={`${styles.tab} ${s.id === section ? styles.tabActive : ''}`.trim()}
              onClick={() => setSection(s.id)}
            >
              {s.icon}
              {s.label}
            </button>
          ))}
        </div>
        <div className={styles.controls}>
          <div className={styles.search}>
            <SearchField value={draft} onChange={setDraft} placeholder="Search places" />
          </div>
          <div className={styles.dd}>
            <Dropdown options={CATEGORY_OPTIONS} value={categoryLabel(category)} onChange={(label) => setCategory(categoryId(label))} />
          </div>
          <div className={styles.dd}>
            <Dropdown
              options={SORT_OPTIONS.map((o) => o.label)}
              value={SORT_OPTIONS.find((o) => o.value === sort)?.label ?? 'Most active'}
              onChange={(label) => setSort(SORT_OPTIONS.find((o) => o.label === label)?.value ?? 'most_active')}
            />
          </div>
          {headExtra}
        </div>
      </div>

      <div className={styles.panel}>
        {loading ? (
          <div className={styles.center}>
            <Spinner size={34} />
          </div>
        ) : error ? (
          <EmptyState variant="inline" tone="error" title="Couldn't load places" subtitle={error} />
        ) : list.length === 0 ? (
          <EmptyState
            variant="inline"
            icon={section === 'favourites' ? <Heart size={34} /> : <Pin size={34} />}
            title={section === 'favourites' ? 'No favourites yet' : section === 'my' ? 'No places of your own' : 'No results'}
            subtitle={
              section === 'favourites'
                ? 'Sign in and favourite places to find them here.'
                : section === 'my'
                  ? 'Places you own will show up here.'
                  : 'Nothing matched your search.'
            }
          />
        ) : (
          renderGrid(mainList)
        )}
      </div>
    </>
  )
}
