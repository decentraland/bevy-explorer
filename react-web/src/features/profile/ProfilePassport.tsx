// Passport — the full-screen profile view opened from the chat profile menu's
// "View Profile" (Figma node 8337-…). Header (name/address/copy/mutuals + FRIEND /
// ADD FRIEND), OVERVIEW / BADGES / PHOTOS tabs, a 3D avatar rendered into an engine
// cutout (same pattern as the Backpack), then badges + about-me + fields + links.
//
// NOTE: two backend follow-ups for OTHER users — (1) the bridge must fetch their rich
// profile (badges/info/equipped/mutuals) by address; (2) the engine cutout currently
// renders the LOCAL avatar, so a per-user render is needed. The 2D picture is shown
// behind the cutout as a fallback meanwhile.

import { useState } from 'react'
import { Avatar } from '../../design'
import { nameColor, shortAddr, splitName } from '../../lib/identity'
import { EngineViewport } from '../engine/EngineViewport'
import type { Badge, Profile, ProfileInfo } from '../../engine/protocol'
import styles from './ProfilePassport.module.css'

type Tab = 'overview' | 'badges' | 'photos'

const FIELD_LABELS: { key: keyof ProfileInfo; label: string }[] = [
  { key: 'gender', label: 'Gender' },
  { key: 'birthdate', label: 'Birth Date' },
  { key: 'pronouns', label: 'Pronouns' },
  { key: 'relationship', label: 'Relationship Status' },
  { key: 'language', label: 'Language' },
  { key: 'profession', label: 'Profession' },
  { key: 'employment', label: 'Employment Status' },
  { key: 'hobby', label: 'Favorite Hobby' },
  { key: 'realName', label: 'Real Name' }
]

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
    <div className={styles.badge} title={badge.name}>
      {badge.image ? <img src={badge.image} alt={badge.name} /> : <span className={styles.badgePlaceholder} />}
    </div>
  )
}

export function ProfilePassport({
  profile,
  isFriend = false,
  useEngineViewport = false,
  onAddFriend,
  onClose,
  setEngineViewport
}: {
  profile: Profile
  isFriend?: boolean
  /** Render the live 3D avatar into the engine cutout (only when the engine can render
   *  THIS user — currently the local player). Otherwise the 2D avatar is shown. */
  useEngineViewport?: boolean
  onAddFriend?: (address: string) => void
  onClose: () => void
  setEngineViewport?: (region: 'map' | 'avatarPreview', rect: { x: number; y: number; width: number; height: number } | null) => void
}): React.JSX.Element {
  const [tab, setTab] = useState<Tab>('overview')
  const { base, tag } = splitName(profile.name)
  const claimed = profile.hasClaimedName
  const fields = FIELD_LABELS.filter(({ key }) => profile.info?.[key])
  const hasOverview =
    !!profile.description || fields.length > 0 || (profile.badges?.length ?? 0) > 0 || (profile.links?.length ?? 0) > 0

  return (
    <div className={styles.overlay}>
      <div className={styles.panel}>
        {/* --- header --- */}
        <header className={styles.head}>
          <div className={styles.idblock}>
            <div className={styles.nameRow}>
              <span className={styles.name}>{base}</span>
              {claimed && <Verified />}
              {tag && <span className={styles.tag}>{tag}</span>}
              <CopyButton value={profile.name} label="name" />
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
            <button type="button" className={`${styles.friendBtn} ${isFriend ? styles.isFriend : ''}`.trim()} onClick={() => !isFriend && onAddFriend?.(profile.address)} disabled={isFriend}>
              {isFriend ? 'FRIEND' : 'ADD FRIEND'}
            </button>
            <button type="button" className={styles.close} aria-label="Close" onClick={onClose}>×</button>
          </div>
        </header>

        {/* --- tabs --- */}
        <nav className={styles.tabs}>
          {(['overview', 'badges', 'photos'] as Tab[]).map((t) => (
            <button key={t} type="button" className={`${styles.tab} ${tab === t ? styles.tabActive : ''}`.trim()} onClick={() => setTab(t)}>
              {t.toUpperCase()}
            </button>
          ))}
        </nav>

        <div className={styles.body}>
          {/* --- left: the avatar. Live 3D engine cutout (like the Backpack) only when the
                  engine can render THIS user; otherwise their 2D avatar. --- */}
          <div className={styles.avatarCol}>
            {useEngineViewport && setEngineViewport ? (
              <EngineViewport region="avatarPreview" report={setEngineViewport} />
            ) : profile.bodyImage ? (
              <img className={styles.body} src={profile.bodyImage} alt={base} />
            ) : (
              <Avatar src={profile.picture} name={base} color={nameColor(profile.address || profile.name)} size={180} status="online" />
            )}
          </div>

          {/* --- right: tab content --- */}
          <div className={styles.content}>
            {tab === 'overview' && !hasOverview && (
              <div className={styles.empty}>This profile has no details to show yet.</div>
            )}
            {tab === 'overview' && hasOverview && (
              <>
                {profile.badges && profile.badges.length > 0 && (
                  <section className={styles.card}>
                    <h2 className={styles.cardTitle}>Badges</h2>
                    <div className={styles.badgeRow}>
                      {profile.badges.map((b) => <BadgeTile key={b.id} badge={b} />)}
                    </div>
                  </section>
                )}
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
    </div>
  )
}
