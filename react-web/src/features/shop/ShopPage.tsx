// Shop — the Explore panel's Shop section (unity-explorer ExploreSections.Shop): browse wearables and
// emotes on sale; Buy opens the item on the web marketplace, where the wallet transaction happens.

import { useCallback, useEffect, useRef, useState } from 'react'
import { BrowseControl, BrowseLoading, BrowsePanel, BrowseToolbar, Button, Dropdown, EmptyState, EquippedItemCard, SearchField, Tabs, type TabItem } from '../../design'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { ProfileState, ShopState } from '../session/useEngineSession'
import { fetchShop, marketplaceUrl, shopItemPrice, type ShopCategory, type ShopItem, type ShopSort } from './shopApi'
import styles from './ShopPage.module.css'

const SECTIONS: TabItem<ShopCategory>[] = [
  { id: 'wearable', label: 'Wearables' },
  { id: 'emote', label: 'Emotes' }
]
const SORTS: { value: ShopSort; label: string }[] = [
  { value: 'recently_listed', label: 'Recently listed' },
  { value: 'recently_sold', label: 'Recently sold' },
  { value: 'cheapest', label: 'Cheapest' },
  { value: 'most_expensive', label: 'Most expensive' }
]
const SEARCH_DEBOUNCE_MS = 350

export function ShopPage({
  shop,
  profile,
  onNavigate
}: {
  shop: ShopState
  profile: ProfileState
  onNavigate: (page: string) => void
}): React.JSX.Element | null {
  const [category, setCategory] = useState<ShopCategory>('wearable')
  const [sortBy, setSortBy] = useState<ShopSort>('recently_listed')
  const [draft, setDraft] = useState('')
  const [search, setSearch] = useState('')
  // Results per query (tab|sort|search): a query seen before shows its last results, dimmed until the
  // refetch answers; a new one shows the spinner.
  const [results, setResults] = useState<ReadonlyMap<string, { items: ShopItem[]; total: number }>>(() => new Map())
  // The query whose stored results are current; any other query's are stale.
  const [freshKey, setFreshKey] = useState<string | null>(null)
  const [skip, setSkip] = useState(0)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => setSearch(draft), SEARCH_DEBOUNCE_MS)
    return () => {
      if (timer.current) clearTimeout(timer.current)
    }
  }, [draft])

  // A new query starts from the first page.
  useEffect(() => setSkip(0), [category, sortBy, search])

  const key = `${category}|${sortBy}|${search}`
  useEffect(() => {
    if (!shop.open) return
    const ctrl = new AbortController()
    setLoading(true)
    setError(null)
    if (skip === 0) setFreshKey(null)
    fetchShop({ category, sortBy, search, skip }, ctrl.signal).then(
      (page) => {
        setResults((prev) => {
          const kept = skip === 0 ? [] : (prev.get(key)?.items ?? [])
          return new Map(prev).set(key, { items: [...kept, ...page.items], total: page.total })
        })
        setFreshKey(key)
        setLoading(false)
      },
      (e: unknown) => {
        if (ctrl.signal.aborted) return
        setError(e instanceof Error ? e.message : 'Could not reach the marketplace')
        setLoading(false)
      }
    )
    return () => ctrl.abort()
  }, [shop.open, category, sortBy, search, key, skip, attempt])

  const retry = useCallback(() => setAttempt((n) => n + 1), [])

  if (!shop.open) return null
  const items = results.get(key)?.items ?? []
  const total = results.get(key)?.total ?? 0
  const stale = freshKey !== key
  const p = profile.data
  return (
    <MainMenuShell
      active="shop"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={shop.toggle}
    >
      <BrowseToolbar tabs={<Tabs items={SECTIONS} value={category} onChange={setCategory} aria-label="Shop sections" />}>
        <BrowseControl size="search">
          <SearchField value={draft} onChange={setDraft} placeholder="Search the shop" />
        </BrowseControl>
        <BrowseControl size="select">
          <Dropdown
            options={SORTS.map((s) => s.label)}
            value={SORTS.find((s) => s.value === sortBy)?.label ?? SORTS[0].label}
            onChange={(label) => setSortBy(SORTS.find((s) => s.label === label)?.value ?? 'recently_listed')}
          />
        </BrowseControl>
      </BrowseToolbar>
      <BrowsePanel>
        {error ? (
          <EmptyState variant="inline" tone="error" title="Couldn't load the shop" subtitle={error} actions={[{ label: 'Retry', onClick: retry }]} />
        ) : loading && items.length === 0 ? (
          <BrowseLoading />
        ) : items.length === 0 ? (
          <EmptyState variant="inline" title="Nothing on sale here" subtitle={search ? 'Nothing matched your search.' : 'Check back soon.'} />
        ) : (
          <>
            <div className={`${styles.grid} ${stale ? styles.stale : ''}`.trim()} aria-busy={stale}>
              {items.map((item) => (
                <EquippedItemCard
                  key={item.id}
                  thumbnail={item.thumbnail}
                  name={item.name}
                  rarity={item.rarity}
                  price={shopItemPrice(item)}
                  shopUrl={marketplaceUrl(item)}
                  shopLabel="Buy"
                />
              ))}
            </div>
            {!stale && items.length < total && (
              <div className={styles.more}>
                <Button variant="secondary" disabled={loading} onClick={() => setSkip(items.length)}>
                  {loading ? 'Loading…' : 'Load more'}
                </Button>
              </div>
            )}
          </>
        )}
      </BrowsePanel>
    </MainMenuShell>
  )
}
