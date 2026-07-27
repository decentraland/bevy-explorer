// React Backpack — full-screen page inside MainMenuShell, matching the Unity backpack:
// live avatar preview (engine cutout) on the left, then a content panel with a category
// column (each tile shows the equipped item for that body part), a paginated 4-col item
// grid, and a right-hand detail panel. Wearables + Emotes tabs. Data via the bridge relay
// of fetchWearablesPage; equipping goes back through setAvatar.

import { useEffect, useMemo, useState } from 'react'
import { WearableCard, type Rarity } from '../../design'
import { catalystThumbUrl, rarityColor } from '../../lib/identity'
import { CatalystImg } from '../../components/CatalystImg'
import { CategoryIcon } from './categoryIcons'
import { EngineViewport } from '../engine/EngineViewport'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { Emote, Outfit, Wearable } from '../../engine/protocol'
import type { BackpackState, EmotesState, ProfileState } from '../session/useEngineSession'
import styles from './BackpackPage.module.css'

const PAGE_SIZE = 16
const NO_DESC = 'This wearable does not have a description set.'

const RARITIES: Rarity[] = ['base', 'common', 'uncommon', 'rare', 'epic', 'legendary', 'mythic', 'unique', 'exotic']
const RARITY_RANK: Record<string, number> = Object.fromEntries(RARITIES.map((r, i) => [r, i]))
const MARKETPLACE_URL = 'https://decentraland.org/marketplace/'
function asRarity(r?: string): Rarity {
  const k = (r ?? '').toLowerCase()
  return RARITIES.find((x) => x === k) ?? 'base'
}
function humanize(s: string): string {
  return s.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

const PAGE_BUTTONS = 5

// A sliding window of consecutive page numbers (mirrors bevy-ui-scene's pagination-util): the
// current page stays centered once past the first half, and the window clamps at both ends so it
// never runs off. All indices 0-based.
export function pageWindow(current: number, count: number): number[] {
  const size = Math.min(PAGE_BUTTONS, count)
  const half = Math.floor(PAGE_BUTTONS / 2)
  const start = current > half ? Math.min(current - half, count - size) : 0
  return Array.from({ length: size }, (_, i) => start + i)
}

// The 18 equipable slot categories, ordered like Unity's Backpack prefab (body parts grouped
// head→body→accessories). NOT included: 'head' — it exists in the schemas only as a hide/replace
// TARGET (wearables can `hides: ["head"]`); nothing is published with category "head", so Unity's
// slot column omits it (its NftCategoryIcons has a glyph for cards, but no AvatarSlot).
const CATEGORY_ORDER = [
  'body_shape', 'hair', 'eyebrows', 'eyes', 'mouth', 'facial_hair',
  'upper_body', 'hands_wear', 'lower_body', 'feet',
  'hat', 'eyewear', 'mask', 'tiara', 'top_head', 'earring', 'helmet', 'skin'
]

// Categories that must always keep something equipped, so their slot shows no unequip button —
// mirrors Unity's IsUnequippable gate (BackpackGridController: not body_shape/eyes/eyebrows/mouth).
const REQUIRED_CATEGORIES = new Set(['body_shape', 'eyes', 'eyebrows', 'mouth'])

function CategoryTile({
  cat,
  active,
  equipped,
  onClick,
  onUnequip
}: {
  cat: string
  active: boolean
  equipped?: Wearable
  onClick: () => void
  /** Present only when the slot holds a removable equipped item — renders the hover unequip button. */
  onUnequip?: () => void
}): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  return (
    <button
      type="button"
      className={`${styles.catTile} ${active ? styles.catActive : ''}`.trim()}
      title={humanize(cat)}
      aria-label={humanize(cat)}
      onClick={onClick}
    >
      {equipped?.thumbnail && !failed ? (
        <img className={styles.catThumb} src={equipped.thumbnail} alt="" onError={() => setFailed(true)} />
      ) : (
        <span className={styles.catGlyph}><CategoryIcon category={cat} /></span>
      )}
      {onUnequip != null && (
        <span
          className={styles.catUnequip}
          role="button"
          aria-label={`Unequip ${humanize(cat)}`}
          title="Unequip"
          onClick={(e) => { e.stopPropagation(); onUnequip() }}
        >
          ✕
        </span>
      )}
    </button>
  )
}

function DetailPanel({ item }: { item: Wearable | Emote | null }): React.JSX.Element {
  if (!item) {
    return (
      <aside className={styles.detail}>
        <div className={styles.detailEmpty}>No item selected</div>
      </aside>
    )
  }
  const rarity = item.rarity ?? 'base'
  const category = 'category' in item ? item.category : 'emote'
  return (
    <aside className={styles.detail}>
      <div className={styles.detailThumb} style={{ background: `radial-gradient(circle at 50% 35%, ${rarityColor(rarity)}, rgba(0,0,0,0.35))` }}>
        <CatalystImg src={item.thumbnail} urn={item.urn} />
      </div>
      <div className={styles.detailName}>{item.name}</div>
      <div className={styles.detailRarity} style={{ background: rarityColor(rarity) }}>
        {humanize(rarity)}
      </div>
      <div className={styles.detailMetaRow}>
        <span className={styles.detailMetaLabel}>CATEGORY</span>
        <span className={styles.detailMetaValue}>{humanize(category)}</span>
      </div>
      <div className={styles.detailDescLabel}>DESCRIPTION</div>
      <div className={styles.detailDesc}>{NO_DESC}</div>
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

function FilterIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
      <path d="M4 6h16M7 12h10M10 18h4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}

// Header icons (replace the tofu unicode glyphs ☰ ⌂ ⬚ that rendered inconsistently across fonts).
function GridIcon({ size = 15 }: { size?: number }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true">
      <rect x="4" y="4" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="2" />
      <rect x="14" y="4" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="2" />
      <rect x="4" y="14" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="2" />
      <rect x="14" y="14" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="2" />
    </svg>
  )
}
function BookmarkIcon({ size = 15 }: { size?: number }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true">
      <path d="M6 4h12v16l-6-4-6 4V4z" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
    </svg>
  )
}
function BagIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" aria-hidden="true">
      <path d="M6 7h12l-1 13H7L6 7z" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
      <path d="M9 7V6a3 3 0 0 1 6 0v1" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
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
  const [tab, setTab] = useState<'wearables' | 'emotes'>(initialTab)
  const [section, setSection] = useState<'categories' | 'outfits'>('categories')
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
  const [sortBy, setSortBy] = useState<'rarity' | 'name'>('rarity')
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('desc')
  const [collectiblesOnly, setCollectiblesOnly] = useState(false)

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
          : dir * ((RARITY_RANK[a.rarity ?? 'base'] ?? 0) - (RARITY_RANK[b.rarity ?? 'base'] ?? 0))
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

  // Explicit equip/unequip (the hover pill) — persists to the profile, then drops the preview
  // override so the avatar follows the (now updated) profile.
  const toggleEquip = (w: Wearable): void => {
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
    section === 'outfits' && outfitSlot !== null
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
      <div className={styles.layout}>
        {/* Top bar: title + Wearables/Emotes pills (left), Filter & Search (right). */}
        <div className={styles.head}>
          <h1 className={styles.title}>Backpack</h1>
          <div className={styles.tabs}>
            <button type="button" className={`${styles.tab} ${tab === 'wearables' ? styles.tabActive : ''}`.trim()} onClick={() => setTab('wearables')}>
              Wearables
            </button>
            <button type="button" className={`${styles.tab} ${tab === 'emotes' ? styles.tabActive : ''}`.trim()} onClick={() => setTab('emotes')}>
              Emotes
            </button>
          </div>
          <div className={styles.filterWrap}>
            <button type="button" className={`${styles.filterBtn} ${showFilter ? styles.filterBtnOpen : ''}`.trim()} onClick={() => setShowFilter((s) => !s)}>
              <FilterIcon /> FILTER &amp; SORT
            </button>
            {showFilter && (
              <div className={styles.filterPop}>
                <span className={styles.filterLabel}>Sort by</span>
                <div className={styles.filterRow}>
                  <button type="button" className={`${styles.filterOpt} ${sortBy === 'rarity' ? styles.filterOptActive : ''}`.trim()} onClick={() => setSortBy('rarity')}>Rarity</button>
                  <button type="button" className={`${styles.filterOpt} ${sortBy === 'name' ? styles.filterOptActive : ''}`.trim()} onClick={() => setSortBy('name')}>Name</button>
                </div>
                <span className={styles.filterLabel}>Order</span>
                <div className={styles.filterRow}>
                  <button type="button" className={`${styles.filterOpt} ${sortDir === 'desc' ? styles.filterOptActive : ''}`.trim()} onClick={() => setSortDir('desc')}>{sortBy === 'name' ? 'Z – A' : 'Rarest'}</button>
                  <button type="button" className={`${styles.filterOpt} ${sortDir === 'asc' ? styles.filterOptActive : ''}`.trim()} onClick={() => setSortDir('asc')}>{sortBy === 'name' ? 'A – Z' : 'Common'}</button>
                </div>
                <label className={styles.filterCheck}>
                  <input type="checkbox" checked={collectiblesOnly} onChange={(e) => setCollectiblesOnly(e.target.checked)} />
                  Collectibles only
                </label>
              </div>
            )}
          </div>
          <input className={styles.search} value={query} onChange={(e) => { setQuery(e.target.value); setPage(0) }} placeholder="Search item" />
        </div>

        <div className={styles.main}>
          {/* Left: live avatar preview (engine renders into this transparent cutout). */}
          <div className={styles.preview}>
            <EngineViewport region="avatarPreview" report={setEngineViewport} />
          </div>

          {/* Opaque panel area (content + detail) — covers the world; only the preview
              column stays transparent so the engine avatar shows through. */}
          <div className={styles.panelArea}>
          {/* Centre: content panel. */}
          <div className={styles.content}>
            <div className={styles.contentHead}>
              <div className={styles.sectionTabs}>
                <button type="button" className={`${styles.sectionTab} ${section === 'categories' ? styles.sectionActive : ''}`.trim()} onClick={() => setSection('categories')}>
                  <GridIcon /> CATEGORIES
                </button>
                <button type="button" className={`${styles.sectionTab} ${section === 'outfits' ? styles.sectionActive : ''}`.trim()} onClick={() => setSection('outfits')}>
                  <BookmarkIcon /> SAVED OUTFITS
                </button>
              </div>
              <button type="button" className={styles.marketplace} onClick={() => window.open(MARKETPLACE_URL, '_blank', 'noopener,noreferrer')}><BagIcon /> MARKETPLACE</button>
            </div>

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
                  {categories.map((c) => {
                    const eq = equippedByCat.get(c)
                    return (
                      <CategoryTile
                        key={c}
                        cat={c}
                        active={cat === c}
                        equipped={eq}
                        onClick={() => pick(c)}
                        onUnequip={eq != null && !REQUIRED_CATEGORIES.has(c) ? () => toggleEquip(eq) : undefined}
                      />
                    )
                  })}
                </div>
                <div className={styles.gridArea}>
                  <div className={styles.chips}>
                    <button type="button" className={`${styles.chip} ${cat === 'all' ? styles.chipActive : ''}`.trim()} onClick={() => pick('all')}>
                      <GridIcon size={13} /> ALL
                    </button>
                    {cat !== 'all' && (
                      <span className={styles.chipSel} style={{ background: 'var(--accent)' }}>
                        {humanize(cat)}
                      </span>
                    )}
                  </div>
                  {pageItems.length === 0 ? (
                    <div className={styles.empty}>{backpack.loading ? 'Loading…' : 'No wearables.'}</div>
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
                          count={w.count}
                          categoryIcon={<CategoryIcon category={w.category} size={15} />}
                          onClick={() => select(w)}
                          onDoubleClick={() => toggleEquip(w)}
                          onEquip={() => toggleEquip(w)}
                        />
                      ))}
                    </div>
                  )}
                  {/* Server-paginated → potentially hundreds of pages: a sliding window of
                      consecutive page numbers (bevy-ui-scene style). The arrows WRAP — prev on the
                      first page jumps to the last, next on the last wraps to the first. */}
                  {pageCount > 1 && (
                    <div className={styles.pager}>
                      <button type="button" className={styles.pageArrow} aria-label="Previous page" onClick={() => setPage(safePage === 0 ? pageCount - 1 : safePage - 1)}>‹</button>
                      {pageWindow(safePage, pageCount).map((p) => (
                        <button
                          key={p}
                          type="button"
                          className={`${styles.pageNum} ${p === safePage ? styles.pageNumActive : ''}`.trim()}
                          aria-current={p === safePage ? 'page' : undefined}
                          onClick={() => setPage(p)}
                        >
                          {p + 1}
                        </button>
                      ))}
                      <button type="button" className={styles.pageArrow} aria-label="Next page" onClick={() => setPage(safePage >= pageCount - 1 ? 0 : safePage + 1)}>›</button>
                    </div>
                  )}
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
                        <span className={styles.emoteSlotThumb} style={{ background: rarityColor(e?.rarity) }}>
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
                          count={e.count}
                          onClick={() => setSelected(e)}
                          onEquip={() => (e.slot != null ? emotes.equip(e.slot, '') : emotes.equip(emoteSlot, e.urn))}
                        />
                      ))}
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>

          {/* Right: selected-item detail — an outfit's wearables in the Outfits section, else the
              selected wearable/emote. */}
          {selectedOutfit != null ? (
            <OutfitDetailPanel outfit={selectedOutfit} index={outfitSlot as number} />
          ) : (
            <DetailPanel item={section === 'outfits' ? null : selected} />
          )}
          </div>
        </div>
      </div>
    </MainMenuShell>
  )
}
