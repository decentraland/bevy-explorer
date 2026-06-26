// React Backpack — full-screen page inside MainMenuShell, matching the Unity backpack:
// live avatar preview (engine cutout) on the left, then a content panel with a category
// column (each tile shows the equipped item for that body part), a paginated 4-col item
// grid, and a right-hand detail panel. Wearables + Emotes tabs. Data via the bridge relay
// of fetchWearablesPage; equipping goes back through setAvatar.

import { useEffect, useMemo, useState } from 'react'
import { WearableCard, type Rarity } from '../../design'
import { rarityColor } from '../../lib/identity'
import { CatalystImg } from '../../components/CatalystImg'
import { CategoryIcon } from './categoryIcons'
import { EngineViewport } from '../engine/EngineViewport'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { Emote, Wearable } from '../../engine/protocol'
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

// Ordered like Unity's NftCategoryIcons (body parts grouped head→body→accessories).
const CATEGORY_ORDER = [
  'body_shape', 'hair', 'eyebrows', 'eyes', 'mouth', 'facial_hair',
  'upper_body', 'hands_wear', 'lower_body', 'feet',
  'hat', 'eyewear', 'mask', 'head', 'tiara', 'top_head', 'earring', 'helmet', 'skin'
]

function CategoryTile({
  cat,
  active,
  equipped,
  onClick
}: {
  cat: string
  active: boolean
  equipped?: Wearable
  onClick: () => void
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

function FilterIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
      <path d="M4 6h16M7 12h10M10 18h4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}

export function BackpackPage({
  backpack,
  emotes,
  profile,
  onNavigate,
  setEngineViewport
}: {
  backpack: BackpackState
  emotes: EmotesState
  profile: ProfileState
  onNavigate: (page: string) => void
  setEngineViewport: (region: 'map' | 'avatarPreview', rect: { x: number; y: number; width: number; height: number } | null) => void
}): React.JSX.Element | null {
  const [tab, setTab] = useState<'wearables' | 'emotes'>('wearables')
  const [section, setSection] = useState<'categories' | 'outfits'>('categories')
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

  // Categories present in the wearable list, ordered like Unity.
  const categories = useMemo(() => {
    const present = new Set(backpack.list.map((w) => w.category))
    return CATEGORY_ORDER.filter((c) => present.has(c))
  }, [backpack.list])

  const equippedByCat = useMemo(() => {
    const m = new Map<string, Wearable>()
    for (const w of backpack.list) if (w.equipped && !m.has(w.category)) m.set(w.category, w)
    return m
  }, [backpack.list])

  const items = useMemo(() => {
    const dir = sortDir === 'asc' ? 1 : -1
    return backpack.list
      .filter(
        (w) =>
          (cat === 'all' || w.category === cat) &&
          (!query || (w.name ?? '').toLowerCase().includes(query.toLowerCase())) &&
          (!collectiblesOnly || (w.rarity ?? 'base') !== 'base')
      )
      .sort((a, b) =>
        sortBy === 'name'
          ? dir * (a.name ?? '').localeCompare(b.name ?? '')
          : dir * ((RARITY_RANK[a.rarity ?? 'base'] ?? 0) - (RARITY_RANK[b.rarity ?? 'base'] ?? 0))
      )
  }, [backpack.list, cat, query, collectiblesOnly, sortBy, sortDir])
  // Back to page 1 whenever the result set changes.
  useEffect(() => {
    setPage(0)
  }, [cat, query, collectiblesOnly, sortBy, sortDir])
  const pageCount = Math.max(1, Math.ceil(items.length / PAGE_SIZE))
  const safePage = Math.min(page, pageCount - 1)
  const pageItems = items.slice(safePage * PAGE_SIZE, safePage * PAGE_SIZE + PAGE_SIZE)

  // When the Backpack closes, drop any unequipped preview so the avatar reverts to the
  // actual profile (selecting an item must never persist).
  useEffect(() => {
    if (!backpack.open) {
      setSelected(null)
      backpack.preview(null)
    }
  }, [backpack.open, backpack.preview])

  if (!backpack.open) return null

  // The equipped set if `w` were on (same-category item swapped out).
  const equipSetWith = (w: Wearable): string[] =>
    [...backpack.list.filter((x) => x.equipped && x.category !== w.category).map((x) => x.urn), w.urn]

  // Explicit equip/unequip (the hover pill) — persists to the profile, then drops the preview
  // override so the avatar follows the (now updated) profile.
  const toggleEquip = (w: Wearable): void => {
    const next = w.equipped
      ? backpack.list.filter((x) => x.equipped && x.urn !== w.urn).map((x) => x.urn)
      : equipSetWith(w)
    backpack.equip(next)
    backpack.preview(null)
  }
  // Selecting an item (card click) — show it in the detail panel and preview it on the avatar
  // WITHOUT persisting. Closing the Backpack (or equipping) clears this.
  const select = (w: Wearable): void => {
    setSelected(w)
    backpack.preview(w.equipped ? null : equipSetWith(w))
  }
  const pick = (c: string): void => {
    setCat(c)
    setPage(0)
  }

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
                  ☰ CATEGORIES
                </button>
                <button type="button" className={`${styles.sectionTab} ${section === 'outfits' ? styles.sectionActive : ''}`.trim()} onClick={() => setSection('outfits')}>
                  ⌂ SAVED OUTFITS
                </button>
              </div>
              <button type="button" className={styles.marketplace} onClick={() => window.open(MARKETPLACE_URL, '_blank', 'noopener,noreferrer')}>⬚ MARKETPLACE</button>
            </div>

            {tab === 'wearables' ? (
              <div className={styles.catalog}>
                {section === 'categories' && (
                  <div className={styles.catColumn}>
                    {categories.map((c) => (
                      <CategoryTile key={c} cat={c} active={cat === c} equipped={equippedByCat.get(c)} onClick={() => pick(c)} />
                    ))}
                  </div>
                )}
                <div className={styles.gridArea}>
                  <div className={styles.chips}>
                    <button type="button" className={`${styles.chip} ${cat === 'all' ? styles.chipActive : ''}`.trim()} onClick={() => pick('all')}>
                      ∞ ALL
                    </button>
                    {cat !== 'all' && (
                      <span className={styles.chipSel} style={{ background: 'var(--accent)' }}>
                        {humanize(cat)}
                      </span>
                    )}
                  </div>
                  {pageItems.length === 0 ? (
                    <div className={styles.empty}>No wearables.</div>
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
                          onEquip={() => toggleEquip(w)}
                        />
                      ))}
                    </div>
                  )}
                  {pageCount > 1 && (
                    <div className={styles.pager}>
                      <button type="button" className={styles.pageArrow} disabled={safePage === 0} onClick={() => setPage(safePage - 1)}>‹</button>
                      {Array.from({ length: pageCount }, (_, i) => (
                        <button key={i} type="button" className={`${styles.pageNum} ${i === safePage ? styles.pageNumActive : ''}`.trim()} onClick={() => setPage(i)}>
                          {i + 1}
                        </button>
                      ))}
                      <button type="button" className={styles.pageArrow} disabled={safePage >= pageCount - 1} onClick={() => setPage(safePage + 1)}>›</button>
                    </div>
                  )}
                </div>
              </div>
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
                  {emotes.list.length === 0 ? (
                    <div className={styles.empty}>No emotes.</div>
                  ) : (
                    <div className={styles.grid}>
                      {emotes.list.map((e) => (
                        <WearableCard
                          key={e.urn}
                          thumbnail={e.thumbnail}
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

          {/* Right: selected-item detail. */}
          <DetailPanel item={selected} />
          </div>
        </div>
      </div>
    </MainMenuShell>
  )
}
