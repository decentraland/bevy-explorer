// React Backpack — full-screen page inside MainMenuShell, matching the Unity backpack:
// live avatar preview (engine cutout) on the left, then a content panel with a category
// column (each tile shows the equipped item for that body part), a paginated 4-col item
// grid, and a right-hand detail panel. Wearables + Emotes tabs. Data via the bridge relay
// of fetchWearablesPage; equipping goes back through setAvatar.

import { useEffect, useMemo, useState } from 'react'
import { Button, Chip, ColorPicker, MaskIcon, OptionMenu, Pager, SearchField, Tabs, WearableCard, rarityPanel, rarityTile, type Rarity, type TabItem } from '../../design'
import allIcon from '../../assets/category-icons/all.png'
import arrowLeftIcon from '../../assets/backpack/icon-arrow-left.webp'
import emptySearchArt from '../../assets/backpack/empty-search.webp'
import emptyCategoryArt from '../../assets/backpack/empty-category.webp'
import closeIcon from '../../assets/backpack/icon-close.webp'
import wearablesIcon from '../../assets/backpack/icon-wearables.webp'
import emotesIcon from '../../assets/backpack/icon-emotes.webp'
import filterIcon from '../../assets/backpack/icon-filter.webp'
import arrowDownIcon from '../../assets/backpack/icon-arrow-down.webp'
import categoriesIcon from '../../assets/backpack/icon-categories.webp'
import outfitsIcon from '../../assets/backpack/icon-outfits.webp'
import marketplaceIcon from '../../assets/backpack/icon-marketplace.webp'
import { COLOR_LABEL, COLOR_PRESETS, COLOR_TARGET } from './avatarColors'
import { isCompatible } from '../../engine/bodyShape'
import { catalystThumbUrl } from '../../lib/identity'
import { CatalystImg } from '../../components/CatalystImg'
import { CategoryIcon } from './categoryIcons'
import { EngineViewport } from '../engine/EngineViewport'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { Emote, Outfit, Wearable } from '../../engine/protocol'
import type { BackpackState, EmotesState, ProfileState } from '../session/useEngineSession'
import styles from './BackpackPage.module.css'

type BackpackTab = 'wearables' | 'emotes'
const BACKPACK_TABS: TabItem<BackpackTab>[] = [
  { id: 'wearables', label: 'Wearables', icon: <MaskIcon src={wearablesIcon} /> },
  { id: 'emotes', label: 'Emotes', icon: <MaskIcon src={emotesIcon} /> }
]

const PAGE_SIZE = 16

type SortKey = 'newest' | 'oldest' | 'rarest' | 'lessRare' | 'nameAZ' | 'nameZA'
const SORTS: { id: SortKey; label: string; orderBy: 'date' | 'rarity' | 'name'; direction: 'asc' | 'desc' }[] = [
  { id: 'newest', label: 'Newest', orderBy: 'date', direction: 'desc' },
  { id: 'oldest', label: 'Oldest', orderBy: 'date', direction: 'asc' },
  { id: 'rarest', label: 'Rarest', orderBy: 'rarity', direction: 'desc' },
  { id: 'lessRare', label: 'Less rare', orderBy: 'rarity', direction: 'asc' },
  { id: 'nameAZ', label: 'Name A-Z', orderBy: 'name', direction: 'asc' },
  { id: 'nameZA', label: 'Name Z-A', orderBy: 'name', direction: 'desc' }
]
type ViewKey = 'all' | 'collectibles'
const VIEWS: { id: ViewKey; label: string }[] = [
  { id: 'all', label: 'All Items' },
  { id: 'collectibles', label: 'Collectibles only' }
]
const NO_DESC = 'This wearable does not have a description set.'

const RARITIES: Rarity[] = ['base', 'common', 'uncommon', 'rare', 'epic', 'legendary', 'mythic', 'unique', 'exotic']
const RARITY_RANK: Record<string, number> = Object.fromEntries(RARITIES.map((r, i) => [r, i]))
const SHOP_URL = 'https://decentraland.org/shop?utm_source=client'
const MARKETPLACE_URL = 'https://decentraland.org/marketplace'

type Section = 'categories' | 'outfits'
const SECTION_TABS: TabItem<Section>[] = [
  { id: 'categories', label: 'Categories', icon: <MaskIcon src={categoriesIcon} /> },
  { id: 'outfits', label: 'Saved Outfits', icon: <MaskIcon src={outfitsIcon} /> }
]
function asRarity(r?: string): Rarity {
  const k = (r ?? '').toLowerCase()
  return RARITIES.find((x) => x === k) ?? 'base'
}
function humanize(s: string): string {
  return s.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

export { pageWindow } from '../../design'

// The 18 equipable slot categories, ordered like Unity's Backpack prefab (body parts grouped
// head→body→accessories). NOT included: 'head' — it exists in the schemas only as a hide/replace
// TARGET (wearables can `hides: ["head"]`); nothing is published with category "head", so Unity's
// slot column omits it (its NftCategoryIcons has a glyph for cards, but no AvatarSlot).
const CATEGORY_ORDER = [
  'body_shape', 'hair', 'eyebrows', 'eyes', 'mouth', 'facial_hair',
  'upper_body', 'hands_wear', 'lower_body', 'feet',
  'hat', 'eyewear', 'earring', 'mask', 'tiara', 'top_head', 'helmet', 'skin'
]
// Slot rows (two per row), with a divider and a 12px gap after the face and body groups.
const SLOT_ROW_Y = [0, 84, 168, 264, 348, 442, 526, 610, 694]
const SLOT_DIVIDER_Y = [256.25, 434.25]

// Categories that must always keep something equipped, so neither their slot nor their grid card offers unequip —
// mirrors Unity's IsUnequippable gate (BackpackGridController: not body_shape/eyes/eyebrows/mouth).
const REQUIRED_CATEGORIES = new Set(['body_shape', 'eyes', 'eyebrows', 'mouth'])

function CategoryTile({
  cat,
  index,
  active,
  equipped,
  onClick,
  onUnequip
}: {
  cat: string
  index: number
  active: boolean
  equipped?: Wearable
  onClick: () => void
  /** Present only when the slot holds a removable equipped item — renders the hover unequip button. */
  onUnequip?: () => void
}): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const filled = equipped != null
  return (
    <button
      type="button"
      className={`${styles.slot} ${active ? styles.slotActive : ''}`.trim()}
      style={{ left: index % 2 === 0 ? 0 : 148, top: SLOT_ROW_Y[Math.floor(index / 2)] }}
      aria-label={humanize(cat)}
      aria-pressed={active}
      onClick={onClick}
    >
      <span className={styles.slotSelected} aria-hidden="true" />
      <span className={styles.slotIcon}><CategoryIcon category={cat} size={39} /></span>
      <span
        className={`${styles.slotThumb} ${filled ? '' : styles.slotEmpty}`.trim()}
        style={filled ? { backgroundImage: `url(${rarityTile(equipped.rarity)})` } : undefined}
      >
        {filled && equipped.thumbnail && !failed && (
          <img className={styles.slotImg} src={equipped.thumbnail} alt="" onError={() => setFailed(true)} />
        )}
        <span className={styles.slotHover} aria-hidden="true" />
      </span>
      <span className={styles.tooltip} role="tooltip">{humanize(cat)}</span>
      {onUnequip != null && (
        <span
          className={styles.slotUnequip}
          role="button"
          aria-label={`Unequip ${humanize(cat)}`}
          onClick={(e) => { e.stopPropagation(); onUnequip() }}
        >
          <MaskIcon src={closeIcon} size={10} />
        </span>
      )}
    </button>
  )
}

function EmptyResults({ search }: { search: boolean }): React.JSX.Element {
  return (
    <div className={styles.emptyState}>
      <img src={search ? emptySearchArt : emptyCategoryArt} alt="" width={100} height={100} />
      <p className={styles.emptyText}>
        {search ? 'You do not have any wearable that meets this category or search criteria.' : 'There are no items in this category.'}
        <br />
        If you want you can find the ideal one for you in the{' '}
        <a className={styles.emptyLink} href={MARKETPLACE_URL} target="_blank" rel="noopener noreferrer">Marketplace</a>.
      </p>
    </div>
  )
}

function DetailPanel({ item }: { item: Wearable | Emote | null }): React.JSX.Element {
  if (!item) {
    return (
      <aside className={`${styles.detail} ${styles.detailEmpty}`} style={{ '--panel-art': `url(${rarityPanel(null)})` } as React.CSSProperties}>
        <span className={styles.detailEmptyText}>No item selected</span>
      </aside>
    )
  }
  const rarity = item.rarity ?? 'base'
  const category = 'category' in item ? item.category : 'emote'
  return (
    <aside className={styles.detail} style={{ '--panel-art': `url(${rarityPanel(rarity)})` } as React.CSSProperties}>
      <div className={styles.detailImage}>
        <CatalystImg src={item.thumbnail} urn={item.urn} />
      </div>
      <div className={styles.detailInfo}>
        <div className={styles.detailName}>
          <CategoryIcon category={category} size={28} />
          <span>{item.name}</span>
        </div>
        <span className={styles.detailRarity} data-rarity={rarity}>{rarity}</span>
        <div className={styles.detailDescLabel}>DESCRIPTION</div>
        <div className={styles.detailDesc}>{NO_DESC}</div>
      </div>
    </aside>
  )
}

// One Saved-Outfits slot: an empty "save current look" tile, or a saved outfit showing a
// composite of its wearable thumbnails with Equip / Delete actions. Clicking a saved slot selects
// it (shows its wearables in the detail panel) without touching the avatar; Equip or a double-click
// persists it. A dot marks the outfit that matches the current look (like an equipped wearable).
function OutfitSlotCard({
  index,
  outfit,
  selected,
  equipped,
  onSelect,
  onSave,
  onEquip,
  onDelete
}: {
  index: number
  outfit: Outfit | null
  selected: boolean
  equipped: boolean
  onSelect: () => void
  onSave: () => void
  onEquip: () => void
  onDelete: () => void
}): React.JSX.Element {
  if (!outfit) {
    return (
      <button type="button" className={styles.outfitEmpty} onClick={onSave} title="Save current look">
        <span className={styles.outfitPlus} aria-hidden="true">+</span>
        <span className={styles.outfitEmptyLabel}>Save Outfit</span>
      </button>
    )
  }
  return (
    <div className={`${styles.outfitCard} ${selected ? styles.outfitCardSel : ''} ${equipped ? styles.outfitEquipped : ''}`.trim()}>
      <button type="button" className={styles.outfitThumbs} onClick={onSelect} onDoubleClick={onEquip} aria-label={`Outfit ${index + 1}`}>
        {outfit.wearables.slice(0, 4).map((u) => (
          <span key={u} className={styles.outfitThumb}><CatalystImg urn={u} /></span>
        ))}
      </button>
      {equipped && <span className={styles.outfitDot} aria-hidden="true" />}
      <div className={styles.outfitActions}>
        <button type="button" className={styles.outfitEquip} onClick={onEquip}>EQUIP</button>
        <button type="button" className={styles.outfitDelete} onClick={onDelete} aria-label={`Delete Outfit ${index + 1}`} title="Delete outfit">✕</button>
      </div>
      <span className={styles.outfitLabel}>Outfit {index + 1}</span>
    </div>
  )
}

// Right-panel detail for a selected saved outfit: all its wearable thumbnails (the composite card
// shows only the first four).
function OutfitDetailPanel({ outfit, index }: { outfit: Outfit; index: number }): React.JSX.Element {
  return (
    <aside className={styles.detail}>
      <div className={styles.detailName}>Outfit {index + 1}</div>
      <div className={styles.outfitDetailGrid}>
        {outfit.wearables.map((u) => (
          <span key={u} className={styles.outfitDetailThumb}><CatalystImg urn={u} /></span>
        ))}
      </div>
    </aside>
  )
}

export function BackpackPage({
  backpack,
  emotes,
  profile,
  onNavigate,
  setEngineViewport,
  initialTab = 'wearables'
}: {
  backpack: BackpackState
  emotes: EmotesState
  profile: ProfileState
  onNavigate: (page: string) => void
  setEngineViewport: (region: 'map' | 'avatarPreview', rect: { x: number; y: number; width: number; height: number } | null) => void
  /** Which tab to open on (e.g. the emote wheel's "Customise [E]" opens 'emotes'). */
  initialTab?: 'wearables' | 'emotes'
}): React.JSX.Element | null {
  const [tab, setTab] = useState<BackpackTab>(initialTab)
  const [section, setSection] = useState<Section>('categories')
  // The saved-outfit slot currently selected (shown in the detail panel; null = none).
  const [outfitSlot, setOutfitSlot] = useState<number | null>(null)
  const [cat, setCat] = useState('all')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(0)
  const [selected, setSelected] = useState<Wearable | Emote | null>(null)
  // The emote wheel slot (0–9) an assigned emote will go into; the left list selects it.
  const [emoteSlot, setEmoteSlot] = useState(1)
  // Filter & sort (client-side, on the loaded catalog).
  const [showFilter, setShowFilter] = useState(false)
  const [sortKey, setSortKey] = useState<SortKey>('newest')
  const [view, setView] = useState<ViewKey>('all')
  const sort = SORTS.find((x) => x.id === sortKey) ?? SORTS[0]
  const sortBy = sort.orderBy
  const sortDir = sort.direction
  const collectiblesOnly = view === 'collectibles'

  // Fixed body-part slots. With a server-paginated grid we don't hold the full catalog, so the
  // category column is the canonical Unity ordering rather than "categories present in the page".
  const categories = CATEGORY_ORDER

  // Equipped item shown in each category slot. Prefer the current page (so equip/unequip reflects
  // optimistically), then fill from the decoupled equipped set for items not on this page.
  const equippedByCat = useMemo(() => {
    const m = new Map<string, Wearable>()
    for (const w of backpack.list) if (w.equipped && !m.has(w.category)) m.set(w.category, w)
    for (const w of backpack.equipped) if (!m.has(w.category)) m.set(w.category, w)
    return m
  }, [backpack.list, backpack.equipped])

  // Debounce the search box before hitting the server.
  const [searchDebounced, setSearchDebounced] = useState('')
  useEffect(() => {
    const t = setTimeout(() => setSearchDebounced(query), 300)
    return () => clearTimeout(t)
  }, [query])
  // Any filter change resets to the first page.
  useEffect(() => {
    setPage(0)
  }, [cat, searchDebounced, collectiblesOnly, sortBy, sortDir])
  // Fetch the current catalog page from the catalyst (wearables tab). Filters/sort are applied
  // server-side; the session drops stale responses via a request id.
  useEffect(() => {
    if (!backpack.open || tab !== 'wearables') return
    backpack.query({
      page,
      pageSize: PAGE_SIZE,
      category: cat === 'all' ? undefined : cat,
      search: searchDebounced || undefined,
      orderBy: sortBy,
      direction: sortDir,
      collectiblesOnly
    })
  }, [backpack.open, tab, page, cat, searchDebounced, sortBy, sortDir, collectiblesOnly, backpack.query])

  const pageCount = Math.max(1, Math.ceil(backpack.total / PAGE_SIZE))
  const safePage = Math.min(page, pageCount - 1)
  const pageItems = backpack.list

  // Emotes share the same search / sort / collectibles filter as wearables (no category — emotes
  // aren't grouped by body part). The wheel slot list on the left is unaffected.
  const emoteItems = useMemo(() => {
    const dir = sortDir === 'asc' ? 1 : -1
    return emotes.list
      .filter((e) => (!query || (e.name ?? '').toLowerCase().includes(query.toLowerCase())) && (!collectiblesOnly || (e.rarity ?? 'base') !== 'base'))
      .sort((a, b) =>
        sortBy === 'name'
          ? dir * (a.name ?? '').localeCompare(b.name ?? '')
          : sortBy === 'rarity'
            ? dir * ((RARITY_RANK[a.rarity ?? 'base'] ?? 0) - (RARITY_RANK[b.rarity ?? 'base'] ?? 0))
            : 0
      )
  }, [emotes.list, query, collectiblesOnly, sortBy, sortDir])

  // Open on the requested tab (the emote wheel's "Customise [E]" requests 'emotes'). Only fires on the
  // open transition / when the request changes, so a manual tab switch while open is preserved.
  useEffect(() => {
    if (backpack.open) setTab(initialTab)
  }, [backpack.open, initialTab])

  // When the Backpack closes, drop any unequipped preview and clear the selection so a reopen
  // starts clean (selecting an item must never persist).
  useEffect(() => {
    if (!backpack.open) {
      setSelected(null)
      setOutfitSlot(null)
      backpack.preview(null)
    }
  }, [backpack.open, backpack.preview])

  // Leaving the Outfits section clears the selected outfit (and its detail panel).
  useEffect(() => {
    if (section !== 'outfits' && outfitSlot !== null) setOutfitSlot(null)
  }, [section, outfitSlot])

  if (!backpack.open) return null

  // The full equipped set = the decoupled equipped list (NOT the current page — the rest of the
  // outfit lives on other pages). The equipped set if `w` were on (same-category item swapped out).
  const equipSetWith = (w: Wearable): string[] =>
    [...backpack.equipped.filter((x) => x.category !== w.category).map((x) => x.urn), w.urn]

  // Explicit equip/unequip (the hover pill) — changes the Backpack's look (deployed when it closes),
  // then drops the preview override so the avatar follows the (now updated) look.
  const toggleEquip = (w: Wearable): void => {
    if (w.equipped && REQUIRED_CATEGORIES.has(w.category)) return
    const next = w.equipped
      ? backpack.equipped.filter((x) => x.urn !== w.urn).map((x) => x.urn)
      : equipSetWith(w)
    backpack.equip(next)
    backpack.preview(null)
  }
  // Selecting an item (card click) — show it in the detail panel only. It is NOT equipped, and the
  // avatar is left untouched; equipping is an explicit action (the card's EQUIP pill or a double-click).
  const select = (w: Wearable): void => {
    setSelected(w)
  }
  // Clicking a category filters the grid to it; clicking the already-selected one again clears the
  // filter back to "all" (deselect) — matches Unity's OnSlotButtonPressed toggle.
  const pick = (c: string): void => {
    setCat((prev) => (prev === c ? 'all' : c))
    setPage(0)
  }
  // Select a saved outfit — show its wearables in the detail panel. Like selecting a wearable, this
  // never touches the avatar (the preview keeps showing the current look); equip via EQUIP or a
  // double-click.
  const selectOutfit = (slot: number): void => {
    setOutfitSlot(slot)
    setSelected(null)
  }
  // An outfit matches the current look when its wearable set equals the equipped set. The equipped
  // set holds bare item urns; an outfit may carry token/deployed urns, so compare with the same
  // item-urn matcher used for equipping (u === urn or u startsWith `${urn}:`).
  const outfitMatchesEquipped = (outfit: Outfit): boolean => {
    const eq = backpack.equipped
    if (eq.length === 0 || outfit.wearables.length !== eq.length) return false
    return eq.every((w) => outfit.wearables.some((u) => u === w.urn || u.startsWith(`${w.urn}:`)))
  }
  const selectedOutfit =
    tab === 'wearables' && section === 'outfits' && outfitSlot !== null
      ? backpack.outfits.find((o) => o.slot === outfitSlot)?.outfit ?? null
      : null

  const p = profile.data
  return (
    <MainMenuShell
      active="backpack"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={backpack.toggle}
      transparentBody
    >
      <div className={styles.page}>
        <div className={styles.backdrop} aria-hidden="true" />
        <header className={styles.head}>
          <h1 className={styles.title}>Backpack</h1>
          <Tabs variant="section" className={styles.tabs} items={BACKPACK_TABS} value={tab} onChange={setTab} aria-label="Backpack sections" />
          <div className={styles.filterWrap}>
            <Button variant="light" className={styles.filterBtn} aria-expanded={showFilter} onClick={() => setShowFilter((s) => !s)}>
              <MaskIcon src={filterIcon} size={20} />
              FILTER &amp; SORT
              <MaskIcon src={arrowDownIcon} size={12} />
            </Button>
            {showFilter && (
              <OptionMenu
                className={styles.filterMenu}
                onClose={() => setShowFilter(false)}
                sections={[
                  { label: 'Sort by', options: SORTS, value: sortKey, onChange: (id) => setSortKey(id as SortKey) },
                  { label: 'View', options: VIEWS, value: view, onChange: (id) => setView(id as ViewKey) }
                ]}
              />
            )}
          </div>
          <div className={styles.searchWrap}>
            <SearchField variant="light" value={query} onChange={(v) => { setQuery(v); setPage(0) }} placeholder="Search item" />
          </div>
        </header>

        <div className={styles.stage}>
          {/* The engine draws the avatar preview behind this transparent area. */}
          <div className={styles.preview}>
            <EngineViewport region="avatarPreview" report={setEngineViewport} />
          </div>
          <div className={styles.frame}>
          <section className={`${styles.card} ${tab === 'emotes' ? styles.cardEmotes : ''}`.trim()}>
            {tab === 'wearables' && (
              <div className={styles.contentHead}>
                <Tabs variant="subtab" className={styles.sectionTabs} items={SECTION_TABS} value={section} onChange={setSection} aria-label="Wearables sections" />
                <Button variant="outline" className={styles.shop} onClick={() => window.open(SHOP_URL, '_blank', 'noopener,noreferrer')}>
                  <MaskIcon src={marketplaceIcon} />
                  SHOP
                </Button>
              </div>
            )}

            {tab === 'wearables' ? (
              section === 'outfits' ? (
                <div className={styles.outfits}>
                  <div className={styles.outfitGrid}>
                    {Array.from({ length: backpack.outfitSlots }, (_, i) => {
                      const saved = backpack.outfits.find((o) => o.slot === i)?.outfit ?? null
                      return (
                        <OutfitSlotCard
                          key={i}
                          index={i}
                          outfit={saved}
                          selected={outfitSlot === i}
                          equipped={saved != null && outfitMatchesEquipped(saved)}
                          onSelect={() => { if (saved) selectOutfit(i) }}
                          onSave={() => backpack.saveOutfit(i)}
                          onEquip={() => backpack.equipOutfit(i)}
                          onDelete={() => {
                            backpack.deleteOutfit(i)
                            if (outfitSlot === i) setOutfitSlot(null)
                          }}
                        />
                      )
                    })}
                  </div>
                </div>
              ) : (
              <div className={styles.catalog}>
                <div className={styles.catColumn}>
                  {SLOT_DIVIDER_Y.map((y) => <span key={y} className={styles.slotDivider} style={{ top: y }} aria-hidden="true" />)}
                  {categories.map((c, i) => {
                    const eq = equippedByCat.get(c)
                    return (
                      <CategoryTile
                        key={c}
                        cat={c}
                        index={i}
                        active={cat === c}
                        equipped={eq}
                        onClick={() => pick(c)}
                        onUnequip={eq != null && !REQUIRED_CATEGORIES.has(c) ? () => toggleEquip(eq) : undefined}
                      />
                    )
                  })}
                </div>
                <div className={styles.gridArea}>
                  {(backpack.loading || pageItems.length > 0) && <div className={styles.breadcrumb}>
                    <Chip label="All" icon={<MaskIcon src={allIcon} size={32} />} selected={cat === 'all' && query === ''} onClick={() => { pick('all'); setQuery('') }} />
                    {cat !== 'all' && (
                      <>
                        <span className={styles.crumbArrow} aria-hidden="true"><MaskIcon src={arrowLeftIcon} size={15} /></span>
                        <Chip label={humanize(cat)} icon={<CategoryIcon category={cat} size={32} />} selected onClear={() => pick('all')} clearLabel={`Clear ${humanize(cat)}`} />
                      </>
                    )}
                    {query !== '' && (
                      <>
                        <span className={styles.crumbArrow} aria-hidden="true"><MaskIcon src={arrowLeftIcon} size={15} /></span>
                        <Chip label={query} selected onClear={() => setQuery('')} clearLabel="Clear search" />
                      </>
                    )}
                    {COLOR_TARGET[cat] != null && backpack.colors != null && (
                      <ColorPicker
                        label={COLOR_LABEL[COLOR_TARGET[cat]]}
                        value={backpack.colors[COLOR_TARGET[cat]]}
                        presets={COLOR_PRESETS[COLOR_TARGET[cat]]}
                        onChange={(hex) => backpack.setColor(COLOR_TARGET[cat], hex)}
                      />
                    )}
                  </div>}
                  {backpack.loading ? (
                    <div className={styles.grid} aria-busy="true">
                      {Array.from({ length: PAGE_SIZE }, (_, i) => <span key={i} className={styles.skeleton} />)}
                    </div>
                  ) : pageItems.length === 0 ? (
                    <EmptyResults search={searchDebounced !== ''} />
                  ) : (
                    <div className={styles.grid}>
                      {pageItems.map((w) => (
                        <WearableCard
                          key={w.urn}
                          thumbnail={w.thumbnail}
                          name={w.name}
                          rarity={asRarity(w.rarity)}
                          equipped={w.equipped}
                          selected={selected != null && 'urn' in selected && selected.urn === w.urn}
                          incompatible={!isCompatible(w, backpack.bodyShape)}
                          unequippable={!REQUIRED_CATEGORIES.has(w.category)}
                          categoryIcon={<CategoryIcon category={w.category} size={16} />}
                          onClick={() => select(w)}
                          onDoubleClick={() => { if (!w.equipped && isCompatible(w, backpack.bodyShape)) toggleEquip(w) }}
                          onEquip={() => toggleEquip(w)}
                        />
                      ))}
                    </div>
                  )}
                  {!backpack.loading && pageItems.length > 0 && <Pager className={styles.pager} page={safePage} count={pageCount} onChange={setPage} />}
                </div>
              </div>
              )
            ) : (
              <div className={styles.catalog}>
                {/* Emote wheel slots (numbered 1..0). Click to choose which slot the next emote you
                    pick from the grid will be assigned to. */}
                <div className={styles.slotList}>
                  {Array.from({ length: 10 }, (_, k) => {
                    const num = (k + 1) % 10
                    const e = emotes.list.find((x) => x.slot === num) ?? null
                    return (
                      <button
                        key={num}
                        type="button"
                        className={`${styles.emoteSlot} ${emoteSlot === num ? styles.emoteSlotActive : ''}`.trim()}
                        onClick={() => {
                          setEmoteSlot(num)
                          if (e) setSelected(e)
                        }}
                      >
                        <span className={styles.emoteSlotNum}>{num}</span>
                        <span className={styles.emoteSlotName}>{e?.name ?? 'Empty'}</span>
                        <span className={styles.emoteSlotThumb} data-rarity={e?.rarity ?? 'base'}>
                          {e && <CatalystImg urn={e.urn} />}
                        </span>
                      </button>
                    )
                  })}
                </div>
                <div className={styles.gridArea}>
                  {emoteItems.length === 0 ? (
                    <div className={styles.empty}>{emotes.list.length === 0 ? 'No emotes.' : 'No matches.'}</div>
                  ) : (
                    <div className={styles.grid}>
                      {emoteItems.map((e) => (
                        <WearableCard
                          key={e.urn}
                          thumbnail={e.thumbnail ?? catalystThumbUrl(e.urn)}
                          name={e.name}
                          rarity={asRarity(e.rarity)}
                          equipped={e.slot != null}
                          selected={selected != null && 'urn' in selected && selected.urn === e.urn}
                          onClick={() => setSelected(e)}
                          onEquip={() => (e.slot != null ? emotes.equip(e.slot, '') : emotes.equip(emoteSlot, e.urn))}
                        />
                      ))}
                    </div>
                  )}
                </div>
              </div>
            )}

          {/* Right: selected-item detail — an outfit's wearables in the Outfits section, else the
              selected wearable/emote. */}
          {selectedOutfit != null ? (
            <OutfitDetailPanel outfit={selectedOutfit} index={outfitSlot as number} />
          ) : tab === 'wearables' && section === 'categories' && !backpack.loading && pageItems.length === 0 ? null : (
            <DetailPanel item={tab === 'wearables' && section === 'outfits' ? null : selected} />
          )}
          </section>
          </div>
        </div>
      </div>
    </MainMenuShell>
  )
}
