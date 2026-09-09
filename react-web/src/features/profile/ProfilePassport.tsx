// Passport — the full-screen profile view opened from the chat profile menu's
// "View Profile" (Figma node 8337-…). Header (name/address/copy/mutuals + FRIEND /
// ADD FRIEND), OVERVIEW / BADGES / PHOTOS tabs, the avatar as the catalyst full-body
// snapshot (2D, falling back to the face), then badges + about-me + fields + links.
// (No engine/3D render here — that's only the Backpack's avatar preview.)
//
// NOTE: backend follow-up for OTHER users — the bridge must fetch their rich profile
// (badges/info/mutuals) by address; the 2D picture is the fallback meanwhile.

import { useCallback, useEffect, useRef, useState } from 'react'
import { Avatar, Button, EquippedItemCard, Icon, Pencil, Tooltip, type EquippedItemCardProps } from '../../design'
import { CategoryIcon } from '../backpack/categoryIcons'
import { catalystThumbUrl, nameColor, shortAddr, splitName } from '../../lib/identity'
import type { Badge, Emote, Profile, ProfileEdit, Wearable } from '../../engine/protocol'
import { PROFILE_FIELDS } from './profileFields'
import { ProfileEditForm } from './ProfileEditForm'
import type { Relationship } from '../chat/ProfileCardPresentation'
import styles from './ProfilePassport.module.css'

type Tab = 'overview' | 'badges' | 'photos'

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

function BadgeTile({ badge }: { badge: Badge }): React.JSX.Element {
  return (
    <Tooltip label={badge.tier != null ? `${badge.name} · ${badge.tier}` : badge.name} side="top">
      <div className={styles.badge}>
        {badge.image ? <img src={badge.image} alt={badge.name} /> : <span className={styles.badgePlaceholder} />}
      </div>
    </Tooltip>
  )
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
  onClose,
  onDirtyChange
}: {
  profile: Profile
  /** Relationship of the local user to this profile — drives the header CTA. Hides it entirely for
   *  'incoming' (they requested us — showing ADD FRIEND would fire a duplicate request) and 'blocked'. */
  relationship?: Relationship
  /** Your own passport — hides the friend action (you can't friend yourself). */
  isSelf?: boolean
  /** Own-profile edit mode. Only offered when this is your passport. */
  editing?: PassportEditing
  onAddFriend?: (address: string) => void
  onClose: () => void
  /** Announce unsaved edits, so the popup layer can refuse to close on a stray backdrop click. */
  onDirtyChange?: (dirty: boolean) => void
}): React.JSX.Element {
  const [tab, setTab] = useState<Tab>('overview')
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
  // Optimistic: flip to "Requested" the instant Add Friend is clicked (the sent-list
  // poll catches up a beat later), so the button isn't a no-op visually.
  const [justRequested, setJustRequested] = useState(false)
  const pending = relationship === 'requested' || justRequested
  // (Escape is handled centrally by the popup stack — see popups.tsx.)
  const { base, tag } = splitName(profile.name)
  const claimed = profile.hasClaimedName
  const fields = PROFILE_FIELDS.filter(({ key }) => profile.info?.[key])
  const hasBadges = (profile.badges?.length ?? 0) > 0
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
            {canEdit && !editMode && (
              <button
                type="button"
                className={styles.headBtn}
                onClick={() => {
                  setTab('overview')
                  setEditMode(true)
                }}
              >
                EDIT PROFILE
              </button>
            )}
            {!isSelf && relationship !== 'incoming' && relationship !== 'blocked' &&
              (relationship === 'friend' ? (
                <button type="button" className={`${styles.headBtn} ${styles.headBtnInert}`} disabled>
                  FRIEND
                </button>
              ) : pending ? (
                <button type="button" className={`${styles.headBtn} ${styles.headBtnInert}`} disabled>
                  REQUESTED
                </button>
              ) : (
                <button
                  type="button"
                  className={styles.headBtn}
                  onClick={() => {
                    onAddFriend?.(profile.address)
                    setJustRequested(true)
                  }}
                >
                  ADD FRIEND
                </button>
              ))}
            {canEdit && editMode && (
              <Button variant="primary" disabled={!editStatus.canSave} onClick={() => saveRef.current?.()}>
                {editing.saving ? 'SAVING…' : 'SAVE'}
              </Button>
            )}
            <button type="button" className={styles.close} aria-label="Close" onClick={onClose}>×</button>
          </div>
        </header>

        {/* --- tabs (edit mode is overview-scoped, so they stand down while it's open) --- */}
        {!editMode && (
        <nav className={styles.tabs}>
          {(['overview', 'badges', 'photos'] as Tab[]).map((t) => (
            <button key={t} type="button" className={`${styles.tab} ${tab === t ? styles.tabActive : ''}`.trim()} onClick={() => setTab(t)}>
              {t.toUpperCase()}
            </button>
          ))}
        </nav>
        )}
        {/* Edit mode has no tabs to offer, but it keeps the bar: dropping it shifts the avatar and
            everything below it up by its height, so clicking EDIT PROFILE jumped the whole panel. */}
        {editMode && (
        <div className={styles.tabs}>
          <span className={styles.tabLabel}>EDIT PROFILE</span>
        </div>
        )}

        <div className={styles.body}>
          {/* --- left: the avatar — the catalyst full-body snapshot (Unity-style hero),
                  falling back to the 2D face if the body render isn't available. --- */}
          <div className={styles.avatarCol}>
            {profile.bodyImage ? (
              <img className={styles.avatarImg} src={profile.bodyImage} alt={base} />
            ) : (
              <Avatar src={profile.picture} name={base} color={nameColor(profile.address || profile.name)} size={180} status="online" />
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
                onCancel={() => {
                  editing.dismissError()
                  setEditMode(false)
                }}
                onDismissError={editing.dismissError}
              />
            )}
            {tab === 'overview' && !hasOverview && !editMode && (
              <div className={styles.empty}>This profile has no details to show yet.</div>
            )}
            {tab === 'overview' && hasOverview && (
              <>
                {hasAbout && !editMode && (
                <section className={styles.card}>
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
                {profile.badges && profile.badges.length > 0 && (
                  <section className={styles.card}>
                    <h2 className={styles.cardTitle}>Badges</h2>
                    <div className={styles.badgeRow}>
                      {profile.badges.map((b) => <BadgeTile key={b.id} badge={b} />)}
                    </div>
                  </section>
                )}
              </>
            )}

            {tab === 'badges' && (
              <section className={styles.card}>
                {profile.badges && profile.badges.length > 0 ? (
                  <div className={styles.badgeGrid}>
                    {profile.badges.map((b) => <BadgeTile key={b.id} badge={b} />)}
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
