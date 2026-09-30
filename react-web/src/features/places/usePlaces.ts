// usePlaces — owns the Places browser state (section / search / category / sort) and drives the
// fetch. Re-fetches whenever any of those change; a `cancelled` flag drops any stale response so it
// never clobbers the list. (We don't abort the request: placesApi caches/shares promises across
// consumers, so aborting one would break the others — see placesApi cache note.) Results are
// ordered featured-first, then by user_count (most active), matching the "What's On" design.

import { useEffect, useState } from 'react'
import { getStoredLogin } from '../auth/sso'
import { fetchLiveWorlds, fetchPlaces, fetchWorlds, type DiscoverPlace, type PlacesOrderBy, type PlacesResponse } from './placesApi'

export type PlacesSection = 'all' | 'favourites' | 'my'
export type PlacesSort = 'most_active' | 'like_score_best' | 'created_at' | 'name'

export interface UsePlaces {
  places: DiscoverPlace[]
  /** Worlds with users online right now (for Live Now) — /places only covers Genesis City. */
  liveWorlds: DiscoverPlace[]
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

const PAGE_LIMIT = 48

export function usePlaces(): UsePlaces {
  const [places, setPlaces] = useState<DiscoverPlace[]>([])
  const [liveWorlds, setLiveWorlds] = useState<DiscoverPlace[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  // 'all' = no category filter; 'featured' = points of interest (only_pois).
  const [category, setCategory] = useState('all')
  const [sort, setSort] = useState<PlacesSort>('most_active')
  const [section, setSection] = useState<PlacesSection>('all')

  useEffect(() => {
    let cancelled = false
    fetchLiveWorlds()
      .then((w) => {
        if (!cancelled) setLiveWorlds(w)
      })
      .catch(() => {})
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
    const args = {
      limit: PAGE_LIMIT,
      offset: 0,
      order_by: sort as PlacesOrderBy,
      order: 'desc' as const,
      search: q || undefined
    }

    // A search should find a match whatever's selected — typing "boedo" (a World) shouldn't come back
    // empty. So while searching we query BOTH and merge; otherwise we browse places with the category
    // filter. A failed endpoint degrades to empty rather than failing the whole list.
    const dataOrEmpty = (pr: Promise<PlacesResponse>): Promise<DiscoverPlace[]> => pr.then((r) => r.data).catch(() => [])
    const bothOwnedAndWorlds = (extra: { owner?: string }): Promise<DiscoverPlace[]> =>
      Promise.all([dataOrEmpty(fetchPlaces({ ...args, ...extra })), dataOrEmpty(fetchWorlds({ ...args, ...extra }))]).then(([p, w]) => [...p, ...w])

    // My Places = the signed-in creator's own places + worlds (public owner filter; boedo.dcl.eth etc.).
    const owner = getStoredLogin()?.address
    const run: Promise<DiscoverPlace[]> =
      section === 'my'
        ? owner
          ? bothOwnedAndWorlds({ owner })
          : Promise.resolve([])
        : q
          ? bothOwnedAndWorlds({})
          : fetchPlaces({
              ...args,
              only_pois: category === 'featured' ? true : undefined,
              categories: category === 'all' || category === 'featured' ? undefined : [category]
            }).then((r) => r.data)

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

  return { places, liveWorlds, loading, error, search, setSearch, category, setCategory, sort, setSort, section, setSection }
}
