// The Places browser, matched to the "What's On" design and decentraland.org/places: three stacked sections —
//   1. LIVE NOW    — the 4 busiest destinations, places and worlds (a single full-width row)
//   2. FEATURED    — the places API's curated (highlighted) set
//   3. ALL          — a toolbar (Explore all / Favourites / My places + search + Any Category +
//                     Sort by) over a purple grid panel of every place.
// Live Now + Featured are curated highlights shown only on the default Explore-all view (no search,
// no category filter); searching/filtering collapses to the All grid. Shared by the in-world
// PlacesPage and the post-jump-in PlacesPicker; the host supplies an `onPick(place)` action.

import { useEffect, useMemo, useRef, useState } from 'react'
import { BrowseControl, BrowseLoading, BrowsePanel, BrowseToolbar, DiscoverGrid, Dropdown, EmptyState, Heart, Pin, SearchField, Tabs, type TabItem } from '../../design'
import { PlaceCard } from './PlaceCard'
import { FeaturedCarousel } from './FeaturedCarousel'
import { usePlaces, type PlacesSection, type PlacesSort } from './usePlaces'
import { isHiddenPlace, placePlayers, type DiscoverPlace } from './placesApi'
import styles from './PlacesPage.module.css'

const SECTIONS: TabItem<PlacesSection>[] = [
  { id: 'all', label: 'Explore all', icon: <CompassGlyph /> },
  { id: 'favourites', label: 'Favourites', icon: <Heart size={15} /> },
  { id: 'my', label: 'My places', icon: <Pin size={15} /> }
]

const LIVE_NOW_LIMIT = 4

// "Any Category" dropdown — labels map to the usePlaces category id (featured = only_highlighted).
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
  const { places: list, liveFeed, featured: featuredFeed, loading, error, search, setSearch, category, setCategory, sort, setSort, section, setSection } = usePlaces()
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

  // Each destination shows in ONE section only — Live Now → Featured → the grid — as on decentraland.org/places.
  const liveNow = useMemo(
    () =>
      liveFeed
        .filter((p) => !isHiddenPlace(p) && placePlayers(p) > 0)
        .sort((a, b) => placePlayers(b) - placePlayers(a))
        .slice(0, LIVE_NOW_LIMIT),
    [liveFeed]
  )
  const liveIds = useMemo(() => new Set(liveNow.map((p) => p.id)), [liveNow])
  const featured = useMemo(
    () => featuredFeed.filter((p) => !isHiddenPlace(p) && !liveIds.has(p.id)).sort((a, b) => placePlayers(b) - placePlayers(a)),
    [featuredFeed, liveIds]
  )
  const showHighlights = section === 'all' && category === 'all' && search.trim() === '' && !loading && !error
  // The discover grid shows everything not already surfaced in a highlight section above it.
  // A search or My places shows everything it returns: hiding is for the unfiltered browse only.
  const filtered = search.trim() !== '' || section !== 'all'
  const mainList = useMemo(() => {
    if (filtered) return list
    const kept = list.filter((p) => !isHiddenPlace(p))
    if (!showHighlights) return kept
    const featuredIds = new Set(featured.map((p) => p.id))
    return kept.filter((p) => !liveIds.has(p.id) && !featuredIds.has(p.id))
  }, [list, filtered, showHighlights, liveIds, featured])

  // Live Now is a fixed 4-up row: auto-fill would take the column count from the viewport (3 at
  // 1440px, 7 at 2560px).
  const renderGrid = (items: DiscoverPlace[], columns?: number): React.JSX.Element => (
    <DiscoverGrid columns={columns}>
      {items.map((place) => (
        <PlaceCard key={place.id} place={place} onClick={() => onPick(place)} />
      ))}
    </DiscoverGrid>
  )

  return (
    <>
      {showHighlights && liveNow.length > 0 && (
        <section className={styles.section}>
          <h2 className={styles.sectionTitle}>
            <span className={styles.liveDot} /> Live Now
          </h2>
          {renderGrid(liveNow, 4)}
        </section>
      )}

      {showHighlights && featured.length > 0 && (
        <section className={styles.section}>
          <h2 className={styles.sectionTitle}>Featured Places</h2>
          <FeaturedCarousel places={featured} onPick={onPick} />
        </section>
      )}

      <BrowseToolbar tabs={<Tabs items={SECTIONS} value={section} onChange={setSection} aria-label="Places sections" />}>
        <BrowseControl size="search">
          <SearchField value={draft} onChange={setDraft} placeholder="Search places" />
        </BrowseControl>
        <BrowseControl size="select">
          <Dropdown options={CATEGORY_OPTIONS} value={categoryLabel(category)} onChange={(label) => setCategory(categoryId(label))} />
        </BrowseControl>
        <BrowseControl size="select">
          <Dropdown
            options={SORT_OPTIONS.map((o) => o.label)}
            value={SORT_OPTIONS.find((o) => o.value === sort)?.label ?? 'Most active'}
            onChange={(label) => setSort(SORT_OPTIONS.find((o) => o.label === label)?.value ?? 'most_active')}
          />
        </BrowseControl>
        {headExtra}
      </BrowseToolbar>

      <BrowsePanel>
        {loading ? (
          <BrowseLoading />
        ) : error ? (
          <EmptyState variant="inline" tone="error" title="Couldn't load places" subtitle={error} />
        ) : mainList.length === 0 ? (
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
      </BrowsePanel>
    </>
  )
}
