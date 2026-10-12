// usePlaces — owns the Places browser state (section / search / category / sort) and drives the
// fetches, the way decentraland.org/places does: the grid, the Live Now feed and the Featured set
// are three reads of the places API's /destinations feed. A `cancelled` flag drops stale responses
// (we don't abort: placesApi shares cached promises across consumers).

import { useEffect, useState } from 'react'
import { getLogin } from '../auth/sso'
import {
  BROWSE_ARGS,
  FEATURED_ARGS,
  LIVE_FEED_ARGS,
  fetchDestinations,
  fetchPlaces,
  fetchWorlds,
  type DiscoverPlace,
  type PlacesOrderBy,
  type PlacesResponse
} from './placesApi'

export type PlacesSection = 'all' | 'favourites' | 'my'
export type PlacesSort = 'most_active' | 'like_score_best' | 'created_at' | 'name'

export interface UsePlaces {
  places: DiscoverPlace[]
  /** The busiest destinations (places and worlds) with live presence, for Live Now. */
  liveFeed: DiscoverPlace[]
  /** The places API's curated set, for the Featured rail. */
  featured: DiscoverPlace[]
  loading: boolean
  error: string | null
  search: string
  setSearch: (s: string) => void
  category: string
  setCategory: (c: string) => void
  sort: PlacesSort
  setSort: (s: PlacesSort) => void
  section: PlacesSection
  setSection: (s: PlacesSection) => void
}

const dataOrEmpty = (pr: Promise<PlacesResponse>): Promise<DiscoverPlace[]> => pr.then((r) => r.data).catch(() => [])

export function usePlaces(): UsePlaces {
  const [places, setPlaces] = useState<DiscoverPlace[]>([])
  const [liveFeed, setLiveFeed] = useState<DiscoverPlace[]>([])
  const [featured, setFeatured] = useState<DiscoverPlace[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  // 'all' = no category filter; 'featured' = the curated set (only_highlighted).
  const [category, setCategory] = useState('all')
  const [sort, setSort] = useState<PlacesSort>('most_active')
  const [section, setSection] = useState<PlacesSection>('all')

  useEffect(() => {
    let cancelled = false
    void dataOrEmpty(fetchDestinations(LIVE_FEED_ARGS)).then((d) => !cancelled && setLiveFeed(d))
    void dataOrEmpty(fetchDestinations(FEATURED_ARGS)).then((d) => !cancelled && setFeatured(d))
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    // Favourites needs a signed (authenticated) request to the places API, which we can't sign here
    // yet — leave it empty until the picker hands us the identity for signed-fetch.
    if (section === 'favourites') {
      setPlaces([])
      setLoading(false)
      setError(null)
      return
    }

    let cancelled = false
    setLoading(true)
    setError(null)

    const q = search.trim()
    const categories = category === 'all' || category === 'featured' ? undefined : [category]

    // /destinations only orders by activity; the other sorts read the older per-type endpoints.
    const legacySort = (): Promise<DiscoverPlace[]> => {
      const args = { limit: BROWSE_ARGS.limit, offset: 0, order_by: sort as PlacesOrderBy, order: 'desc' as const, search: q || undefined }
      if (q) return Promise.all([dataOrEmpty(fetchPlaces(args)), dataOrEmpty(fetchWorlds(args))]).then(([p, w]) => [...p, ...w])
      return fetchPlaces({ ...args, only_pois: category === 'featured' ? true : undefined, categories }).then((r) => r.data)
    }

    const run: Promise<DiscoverPlace[]> =
      section === 'my'
        ? getLogin().then((login) => (login ? fetchDestinations({ ...BROWSE_ARGS, owner: login.address }).then((r) => r.data) : []))
        : sort !== 'most_active'
          ? legacySort()
          : fetchDestinations({ ...BROWSE_ARGS, search: q || undefined, categories, only_highlighted: category === 'featured' || undefined }).then((r) => r.data)

    run
      .then((data) => {
        if (cancelled) return
        setPlaces(data)
        setLoading(false)
      })
      .catch((e: unknown) => {
        if (cancelled) return
        setError(e instanceof Error ? e.message : 'Failed to load places')
        setPlaces([])
        setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [search, category, sort, section])

  return { places, liveFeed, featured, loading, error, search, setSearch, category, setCategory, sort, setSort, section, setSection }
}
