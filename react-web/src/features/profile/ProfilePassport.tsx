// Passport — the full-screen profile view opened from the chat profile menu's
// "View Profile" (Figma node 8337-…). Header (name/address/copy/mutuals + FRIEND /
// ADD FRIEND), OVERVIEW / BADGES / PHOTOS tabs, the avatar as the catalyst full-body
// snapshot (2D, falling back to the face), then badges + about-me + fields + links.
// (No engine/3D render here — that's only the Backpack's avatar preview.)
//
// NOTE: backend follow-up for OTHER users — the bridge must fetch their rich profile
// (badges/info/mutuals) by address; the 2D picture is the fallback meanwhile.

import { useCallback, useEffect, useRef, useState } from 'react'
import { Avatar, Button, EquippedItemCard, Icon, Pencil, Tabs, Tooltip, showConfirm, type EquippedItemCardProps, type TabItem } from '../../design'
import { CategoryIcon } from '../backpack/categoryIcons'
import { catalystThumbUrl, shortAddr, splitName, userColor } from '../../lib/identity'
import type { Badge, Emote, Profile, ProfileEdit, Wearable } from '../../engine/protocol'
import { PROFILE_FIELDS } from './profileFields'
import { ProfileEditForm } from './ProfileEditForm'
import type { Relationship } from '../chat/ProfileCardPresentation'
import styles from './ProfilePassport.module.css'

type Tab = 'overview' | 'badges' | 'photos'
const PASSPORT_TABS: TabItem<Tab>[] = (['overview', 'badges', 'photos'] as const).map((t) => ({ id: t, label: t.toUpperCase() }))
// A single selected-but-disabled tab: the same bar, holding the same height, with nothing to switch to.
const EDIT_TAB: TabItem<'edit'>[] = [{ id: 'edit', label: 'EDIT PROFILE', disabled: true }]

function CopyButton({ value, label }: { value: string; label: string }): React.JSX.Element {
  return (
    <button type="button" className={styles.copy} title={`Copy ${label}`} onClick={() => navigator.clipboard?.writeText(value).catch(() => {})}>
      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" aria-hidden="true">
        <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.7" />
        <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
      </svg>
    </button>
  )
}

function Verified(): React.JSX.Element {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" aria-label="verified">
      <defs>
        <linearGradient id="ppv" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ff2d55" />
          <stop offset="1" stopColor="#c640cd" />
        </linearGradient>
      </defs>
      <path d="M8 1l1.7 1.2 2.1-.2 1 1.8 1.9.9-.5 2 .9 1.9-1.6 1.4.1 2.1-2 .6-1.1 1.8-2-.7-2 .7-1.1-1.8-2-.6.1-2.1L1.6 8.6l.9-1.9-.5-2 1.9-.9 1-1.8 2.1.2z" fill="url(#ppv)" />
      <path d="M5.5 8l1.7 1.7L10.8 6" stroke="#fff" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

const badgeLabel = (badge: Badge): string => (badge.tier != null ? `${badge.name} · ${badge.tier}` : badge.name)
// unity-explorer's badge date: month and year ("Oct. 2026").
const monthYear = (ms: number): string => `${new Date(ms).toLocaleDateString('en-US', { month: 'short' })}. ${new Date(ms).getFullYear()}`

// The badge picture in a circle: the 56px tile by default, `className` for the other sizes.
function BadgeIcon({ image, alt, className = '' }: { image?: string; alt: string; className?: string }): React.JSX.Element {
  return (
    <div className={`${styles.badge} ${className}`}>
      {image ? <img src={image} alt={alt} /> : <span className={styles.badgePlaceholder} />}
    </div>
  )
}

// The Overview's compact row: icon only, name on hover.
function BadgeTile({ badge }: { badge: Badge }): React.JSX.Element {
  return (
    <Tooltip label={badgeLabel(badge)} side="top">
      <BadgeIcon image={badge.image} alt={badge.name} />
    </Tooltip>
  )
}

function BadgeProgress({ done, target }: { done: number; target: number }): React.JSX.Element {
  const pct = target > 0 ? Math.min(100, Math.round((done * 100) / target)) : 0
  return (
    <div className={styles.badgeProgress}>
      <div className={styles.badgeBar} role="progressbar" aria-valuenow={done} aria-valuemin={0} aria-valuemax={target}>
        <div className={styles.badgeBarFill} style={{ width: `${pct}%` }} />
      </div>
      <span className={styles.badgeBarValue}>
        {done}/{target}
      </span>
    </div>
  )
}

// The Badges tab's grid card, after unity-explorer's: icon, name, then the unlock month — or,
// on your own passport, how far you are from the next tier (or from a multi-step badge).
function BadgeCard({ badge, isSelf, selected, onSelect }: { badge: Badge; isSelf: boolean; selected: boolean; onSelect: (id: string) => void }): React.JSX.Element {
  const foot =
    isSelf && badge.progress != null ? (
      <>
        {badge.tiers != null && <span className={styles.badgeLabel}>NEXT TIER</span>}
        <BadgeProgress {...badge.progress} />
      </>
    ) : badge.unlockedAt != null ? (
      <time dateTime={new Date(badge.unlockedAt).toISOString()}>{monthYear(badge.unlockedAt)}</time>
    ) : (
      '—'
    )
  return (
    <button
      type="button"
      className={`${styles.badgeCard} ${selected ? styles.picked : ''} ${badge.locked ? styles.badgeLocked : ''}`}
      aria-pressed={selected}
      onClick={() => onSelect(badge.id)}
    >
      <BadgeIcon image={badge.image} alt={badge.name} />
      <div className={styles.badgeName}>{badgeLabel(badge)}</div>
      <div className={styles.badgeFoot}>{foot}</div>
    </button>
  )
}

// The selected badge, in place of the avatar: a big picture, when it was unlocked, what it took,
// and for tiered badges the tiers in a row (picking one shows that tier) plus, on your own
// passport, the next tier's requirement and your progress toward it.
function BadgeDetail({ badge, tierId, isSelf, onSelectTier }: { badge: Badge; tierId?: string; isSelf: boolean; onSelectTier: (id: string) => void }): React.JSX.Element {
  // Other people's passports list only the tiers they have reached, as unity-explorer does.
  const tiers = isSelf ? badge.tiers : badge.tiers?.filter((t) => t.unlockedAt != null)
  const tier = tiers?.find((t) => t.id === tierId)
  const name = tier != null ? `${badge.name} ${tier.name}` : badge.name
  const image = tier?.image ?? badge.image
  const unlockedAt = tier != null ? tier.unlockedAt : badge.unlockedAt
  const locked = tier != null ? tier.unlockedAt == null : badge.locked === true
  const description = tier?.description ?? badge.description
  const nextTier = badge.tiers?.find((t) => t.unlockedAt == null)
  return (
    <div className={styles.badgeDetail}>
      <BadgeIcon image={image} alt={name} className={`${styles.badgeHero} ${locked ? styles.badgeLocked : ''}`} />
      <h2 className={styles.badgeDetailName}>{name}</h2>
      <div className={styles.badgeDetailDate}>{unlockedAt != null ? `Unlocked: ${monthYear(unlockedAt)}` : 'Locked'}</div>
      {description && <p className={styles.badgeDetailDesc}>{description}</p>}
      {tiers != null && tiers.length > 0 && (
        <div className={styles.tierRow}>
          {tiers.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`${styles.tierBtn} ${t.id === tier?.id ? styles.picked : ''} ${t.unlockedAt == null ? styles.badgeLocked : ''}`}
              aria-pressed={t.id === tier?.id}
              aria-label={`${badge.name} ${t.name}`}
              onClick={() => onSelectTier(t.id)}
            >
              <BadgeIcon image={t.image} alt="" className={styles.badgeSm} />
            </button>
          ))}
        </div>
      )}
      {isSelf && badge.progress != null && (
        <div className={styles.badgeNext}>
          {nextTier != null && (
            <>
              <div className={styles.badgeLabel}>NEXT TIER: {nextTier.name}</div>
              <div className={styles.badgeNextDesc}>{nextTier.description}</div>
            </>
          )}
          <BadgeProgress {...badge.progress} />
        </div>
      )}
    </div>
  )
}

/** The tier a badge opens on: the latest reached, else (nothing reached yet) the first. */
function defaultTier(badge: Badge): string | undefined {
  const reached = badge.tiers?.filter((t) => t.unlockedAt != null)
  return (reached != null && reached.length > 0 ? reached[reached.length - 1] : badge.tiers?.[0])?.id
}

// Read-only equipped-item tiles, 6 per row like unity-explorer's passport. No equip affordance
// (this is someone else's passport, or a view-only summary of your own) — instead the SHOP button
// deep-links to the item's shop page. The link is resolved by the bridge (it needs the item's
// on-chain contract + item id); items with no listing — base wearables and emotes — simply show no
// button.
type EquippedTile = EquippedItemCardProps & { key: string }

const wearableTile = (w: Wearable): EquippedTile => ({
  key: w.urn,
  thumbnail: w.thumbnail ?? catalystThumbUrl(w.urn),
  name: w.name,
  rarity: w.rarity,
  shopUrl: w.shopUrl,
  categoryIcon: <CategoryIcon category={w.category} size={15} />
})

const emoteTile = (e: Emote): EquippedTile => ({
  // Keyed by slot too: the same emote can sit in more than one wheel slot (the equipped set is
  // deduped, the wheel isn't), and a duplicate key drops the second tile.
  key: `${e.urn}:${e.slot}`,
  thumbnail: e.thumbnail ?? catalystThumbUrl(e.urn),
  name: e.name,
  rarity: e.rarity,
  shopUrl: e.shopUrl,
  categoryIcon: <Icon name="emotes" size={15} />
})

function EquippedRow({ tiles }: { tiles: EquippedTile[] }): React.JSX.Element {
  return (
    <div className={styles.equippedRow}>
      {tiles.map(({ key, ...tile }) => <EquippedItemCard key={key} {...tile} />)}
    </div>
  )
}

/** Everything the own-profile edit mode needs. Absent = view only, which is every OTHER user's
 *  passport and your own until the session has a profile to edit. */
export interface PassportEditing {
  saving: boolean
  error: string | null
  save: (edit: ProfileEdit) => void
  dismissError: () => void
  /** Open the name editor — its own popup, since picking a claimed NAME is a different shape of
   *  choice from the rest of the form (see NameEditModal). */
  editName: () => void
}

export function ProfilePassport({
  profile,
  relationship = 'none',
  isSelf = false,
  editing,
  onAddFriend,
  onUnfriend,
  onMessage,
  onCancelRequest,
  onAcceptRequest,
  onUnblock,
  onClose,
  onDirtyChange
}: {
  profile: Profile
  /** Relationship of the local user to this profile — drives the header friendship button. */
  relationship?: Relationship
  /** Your own passport — hides the friend action (you can't friend yourself). */
  isSelf?: boolean
  /** Own-profile edit mode. Only offered when this is your passport. */
  editing?: PassportEditing
  onAddFriend?: (address: string) => void
  onUnfriend?: (address: string) => void
  /** Open a DM with this user. */
  onMessage?: (address: string) => void
  onCancelRequest?: (address: string) => void
  onAcceptRequest?: (address: string) => void
  onUnblock?: (address: string) => void
  onClose: () => void
  /** Announce unsaved edits, so the popup layer can refuse to close on a stray backdrop click. */
  onDirtyChange?: (dirty: boolean) => void
}): React.JSX.Element {
  const [tab, setTab] = useState<Tab>('overview')
  // The Badges tab's selection: a badge id and, for tiered badges, a tier id. Both fall back —
  // to the first badge, and to the badge's latest reached tier — so the panel is never empty.
  const [picked, setPicked] = useState<{ badge?: string; tier?: string }>({})
  const badges = profile.badges ?? []
  const unlockedBadges = badges.filter((b) => !b.locked)
  const selectedBadge = badges.find((b) => b.id === picked.badge) ?? badges[0]
  const selectedTier = selectedBadge?.tiers?.some((t) => t.id === picked.tier) ? picked.tier : selectedBadge != null ? defaultTier(selectedBadge) : undefined
  const [editMode, setEditMode] = useState(false)
  const canEdit = isSelf && editing != null
  // SAVE sits in the header rather than at the end of the form: the form is taller than the panel,
  // so a footer button is below the fold and easy to miss entirely.
  const saveRef = useRef<(() => void) | null>(null)
  const [editStatus, setEditStatus] = useState({ dirty: false, canSave: false })
  const onStatusChange = useCallback(
    (s: { dirty: boolean; canSave: boolean }) =>
      setEditStatus((prev) => (prev.dirty === s.dirty && prev.canSave === s.canSave ? prev : s)),
    []
  )
  // Only edit mode has unsaved state; leaving it (save, cancel) clears the guard.
  const unsaved = editMode && editStatus.dirty

  // Leaving edit mode without saving. Asks the same question the popup layer asks when you close
  // the whole passport with work in progress — this is the same loss, by a shorter route.
  const cancelEdit = async (): Promise<void> => {
    if (
      unsaved &&
      !(await showConfirm({
        title: 'Discard changes?',
        body: 'Your edits to this profile will be lost.',
        confirmLabel: 'Discard',
        cancelLabel: 'Keep editing'
      }))
    ) {
      return
    }
    editing?.dismissError()
    setEditMode(false)
  }
  const dirtyCb = useRef(onDirtyChange)
  dirtyCb.current = onDirtyChange
  useEffect(() => {
    dirtyCb.current?.(unsaved)
    return () => dirtyCb.current?.(false)
  }, [unsaved])
  // Leave edit mode only once a save has actually landed: a rejected deploy comes back as an error
  // on `editing`, and closing the form on click would throw away both the error and the user's
  // unsaved text.
  const wasSaving = useRef(false)
  useEffect(() => {
    if (editing == null) return
    if (wasSaving.current && !editing.saving && editing.error == null) setEditMode(false)
    wasSaving.current = editing.saving
  }, [editing])
  // (Escape is handled centrally by the popup stack — see popups.tsx.)
  const { base, tag } = splitName(profile.name)
  const claimed = profile.hasClaimedName
  const fields = PROFILE_FIELDS.filter(({ key }) => profile.info?.[key])
  const hasBadges = unlockedBadges.length > 0
  const hasAbout = !!profile.description || fields.length > 0 || (profile.links?.length ?? 0) > 0
  // The body shape isn't a collectible you can shop for — Unity skips it before filling the grid
  // (EquippedItems_PassportModuleController.SetGridElements). It skips hidden categories too, but
  // that needs each item's hides/replaces metadata, which the equipped set doesn't carry.
  const wearables = (profile.equippedWearables ?? []).filter((w) => w.category !== 'body_shape')
  const hasWearables = wearables.length > 0
  const hasEmotes = (profile.equippedEmotes?.length ?? 0) > 0
  const hasEquipped = hasWearables || hasEmotes
  const hasOverview = hasBadges || hasAbout || hasEquipped

  return (
    // The dimmed scrim + click-outside-to-close are owned by the popup layer (openPassport →
    // PopupHost); this is just the panel. stopPropagation keeps a click inside it (tabs, copy, links)
    // from reaching the scrim and closing the passport.
    <div className={styles.panel} onClick={(e) => e.stopPropagation()}>
        {/* --- header --- */}
        <header className={styles.head}>
          <div className={styles.idblock}>
            <div className={styles.nameRow}>
              <span className={styles.name}>{base}</span>
              {claimed && <Verified />}
              {tag && <span className={styles.tag}>{tag}</span>}
              <CopyButton value={profile.name} label="name" />
              {canEdit && (
                <button type="button" className={styles.iconBtn} aria-label="Edit name" onClick={editing.editName}>
                  <Pencil size={16} />
                </button>
              )}
            </div>
            <div className={styles.addrRow}>
              <span className={styles.addr}>{shortAddr(profile.address)}</span>
              <CopyButton value={profile.address} label="address" />
            </div>
            {profile.mutuals != null && profile.mutuals > 0 && (
              <div className={styles.mutual}>{profile.mutuals} Mutual</div>
            )}
          </div>
          <div className={styles.headActions}>
            {!isSelf && relationship !== 'blocked' && onMessage != null && (
              <button type="button" className={styles.headBtn} onClick={() => onMessage(profile.address)}>
                CHAT
              </button>
            )}
            {!isSelf &&
              (relationship === 'friend' ? (
                <button type="button" className={`${styles.headBtn} ${styles.headBtnSwap}`} onClick={() => onUnfriend?.(profile.address)}>
                  <span className={styles.idle}>FRIEND</span>
                  <span className={styles.hover}>REMOVE FRIEND</span>
                </button>
              ) : relationship === 'requested' ? (
                <button type="button" className={styles.headBtn} onClick={() => onCancelRequest?.(profile.address)}>
                  CANCEL REQUEST
                </button>
              ) : relationship === 'incoming' ? (
                <button type="button" className={styles.headBtn} onClick={() => onAcceptRequest?.(profile.address)}>
                  ACCEPT FRIEND
                </button>
              ) : relationship === 'blocked' ? (
                <button type="button" className={styles.headBtn} onClick={() => onUnblock?.(profile.address)}>
                  UNBLOCK
                </button>
              ) : (
                <button type="button" className={styles.headBtn} onClick={() => onAddFriend?.(profile.address)}>
                  ADD FRIEND
                </button>
              ))}
            {canEdit && editMode && (
              <Button variant="primary" disabled={!editStatus.canSave} onClick={() => saveRef.current?.()}>
                {editing.saving ? 'SAVING…' : 'SAVE'}
              </Button>
            )}
            {canEdit && editMode ? (
              <Button variant="secondary" onClick={() => void cancelEdit()} disabled={editing.saving}>
                CANCEL
              </Button>
            ) : (
              <button type="button" className={styles.close} aria-label="Close" onClick={onClose}>×</button>
            )}
          </div>
        </header>

        {/* --- tabs (edit mode is overview-scoped, so they stand down while it's open) --- */}
        {!editMode && (
          <Tabs variant="underline" className={styles.tabs} items={PASSPORT_TABS} value={tab} onChange={setTab} aria-label="Passport sections" />
        )}
        {/* Edit mode has no tabs to offer, but it keeps the bar: dropping it shifts the avatar and
            everything below it up by its height, so clicking EDIT PROFILE jumped the whole panel. */}
        {editMode && (
          <Tabs variant="underline" className={styles.tabs} items={EDIT_TAB} value="edit" onChange={() => {}} aria-label="Passport sections" />
        )}

        <div className={styles.body}>
          {/* --- left: the avatar — the catalyst full-body snapshot (Unity-style hero),
                  falling back to the 2D face if the body render isn't available. --- */}
          <div className={styles.avatarCol}>
            {tab === 'badges' && !editMode && selectedBadge != null ? (
              <BadgeDetail badge={selectedBadge} tierId={selectedTier} isSelf={isSelf} onSelectTier={(tier) => setPicked({ badge: selectedBadge.id, tier })} />
            ) : profile.bodyImage ? (
              <img className={styles.avatarImg} src={profile.bodyImage} alt={base} />
            ) : (
              <Avatar src={profile.picture} name={base} color={userColor(profile.address, profile.name, profile.hasClaimedName, profile.nameColor)} size={180} status="online" />
            )}
          </div>

          {/* --- right: tab content --- */}
          <div className={styles.content}>
            {/* Edit mode replaces the About card (name, bio, fields and links are exactly what it
                covers) and leaves the equipped/badges sections below it — those are the Backpack's
                to change, not the passport's. */}
            {editMode && editing != null && (
              <ProfileEditForm
                profile={profile}
                saving={editing.saving}
                error={editing.error}
                onSave={editing.save}
                onStatusChange={onStatusChange}
                saveRef={saveRef}
                onDismissError={editing.dismissError}
              />
            )}
            {tab === 'overview' && !hasOverview && !editMode && !canEdit && (
              <div className={styles.empty}>This profile has no details to show yet.</div>
            )}
            {tab === 'overview' && (hasOverview || canEdit) && (
              <>
                {/* Rendered for your own passport even when empty: the pencil in its corner is how
                    profile editing is reached, so an untouched profile still has a way in. */}
                {(hasAbout || canEdit) && !editMode && (
                <section className={styles.card}>
                  {canEdit && (
                    <button
                      type="button"
                      className={styles.cardEdit}
                      aria-label="Edit profile"
                      onClick={() => {
                        setTab('overview')
                        setEditMode(true)
                      }}
                    >
                      <Pencil size={16} />
                    </button>
                  )}
                  {!hasAbout && <p className={styles.about}>Nothing here yet — the pencil adds your details.</p>}
                  {profile.description && (
                    <>
                      <h2 className={styles.cardTitle}>About Me</h2>
                      <p className={styles.about}>{profile.description}</p>
                    </>
                  )}
                  {fields.length > 0 && (
                    <div className={styles.fields}>
                      {fields.map(({ key, label }) => (
                        <div key={key} className={styles.field}>
                          <span className={styles.fieldLabel}>{label}</span>
                          <span className={styles.fieldValue}>{profile.info?.[key]}</span>
                        </div>
                      ))}
                    </div>
                  )}
                  {profile.links && profile.links.length > 0 && (
                    <>
                      <h2 className={styles.cardTitle}>Links</h2>
                      <div className={styles.links}>
                        {profile.links.map((l) => (
                          <a key={l.url} className={styles.link} href={l.url} target="_blank" rel="noreferrer">
                            🔗 {l.title}
                          </a>
                        ))}
                      </div>
                    </>
                  )}
                </section>
                )}
                {hasEquipped && (
                  <section className={styles.card}>
                    {hasWearables && (
                      <>
                        <h2 className={styles.cardTitle}>Equipped Wearables</h2>
                        <EquippedRow tiles={wearables.map(wearableTile)} />
                      </>
                    )}
                    {hasEmotes && (
                      <>
                        <h2 className={styles.cardTitle}>Equipped Emotes</h2>
                        <EquippedRow tiles={(profile.equippedEmotes ?? []).map(emoteTile)} />
                      </>
                    )}
                  </section>
                )}
                {hasBadges && (
                  <section className={styles.card}>
                    <h2 className={styles.cardTitle}>Badges</h2>
                    <div className={styles.badgeRow}>
                      {unlockedBadges.map((b) => <BadgeTile key={b.id} badge={b} />)}
                    </div>
                  </section>
                )}
              </>
            )}

            {tab === 'badges' && (
              <section className={styles.card}>
                {badges.length > 0 ? (
                  <div className={styles.badgeGrid}>
                    {badges.map((b) => (
                      <BadgeCard key={b.id} badge={b} isSelf={isSelf} selected={b.id === selectedBadge?.id} onSelect={(badge) => setPicked({ badge })} />
                    ))}
                  </div>
                ) : (
                  <div className={styles.empty}>No badges yet.</div>
                )}
              </section>
            )}

            {tab === 'photos' && (
              <section className={styles.card}>
                {profile.photos && profile.photos.length > 0 ? (
                  <div className={styles.photoGrid}>
                    {profile.photos.map((src, i) => (
                      <a key={i} href={src} target="_blank" rel="noreferrer"><img src={src} alt="" /></a>
                    ))}
                  </div>
                ) : (
                  <div className={styles.empty}>No photos shared yet.</div>
                )}
              </section>
            )}
          </div>
        </div>
      </div>
  )
}
