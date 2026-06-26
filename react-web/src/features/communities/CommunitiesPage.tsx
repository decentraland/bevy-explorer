// React Communities — a full-screen page inside the shared MainMenuShell (matches the
// Unity Communities screen): a left column with "My Communities" and a main browse grid
// of community cards. Data via the bridge relay of fetchCommunities; joining goes back
// through joinCommunity. We only render what the bridge actually backs (browse + join +
// my-communities derived from role) — invites/create are omitted until wired.

import { useEffect, useMemo, useState } from 'react'
import { Avatar, Button } from '../../design'
import { nameColor } from '../../lib/identity'
import { MainMenuShell } from '../menu/MainMenuShell'
import { CommunityModal } from './CommunityModal'
import type { Community } from '../../engine/protocol'
import type { CommunitiesState, ProfileState } from '../session/useEngineSession'
import styles from './CommunitiesPage.module.css'

function isMember(role: string): boolean {
  return role === 'owner' || role === 'moderator' || role === 'member'
}

function Globe(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="13" height="13" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.7" />
      <path d="M3 12h18M12 3c2.5 2.5 2.5 15 0 18M12 3c-2.5 2.5-2.5 15 0 18" stroke="currentColor" strokeWidth="1.7" />
    </svg>
  )
}

function CommunityCard({ c, onJoin, onOpen }: { c: Community; onJoin: (id: string) => void; onOpen: () => void }): React.JSX.Element {
  const member = isMember(c.role)
  const [failed, setFailed] = useState(false)
  const showImg = c.thumbnail != null && !failed
  return (
    <div className={styles.card} onClick={onOpen} role="button" tabIndex={0}>
      <div className={styles.cover} style={{ background: showImg ? undefined : nameColor(c.id) }}>
        {showImg ? (
          <img className={styles.coverImg} src={c.thumbnail} alt="" onError={() => setFailed(true)} />
        ) : (
          <span className={styles.coverInitial}>{c.name.charAt(0).toUpperCase()}</span>
        )}
      </div>
      <div className={styles.cardBody}>
        <span className={styles.cardName} title={c.name}>{c.name}</span>
        {c.ownerName && <span className={styles.cardOwner}>{c.ownerName}</span>}
        <span className={styles.cardMeta}>
          <Globe /> Public · {c.membersCount} Members
        </span>
        {member ? (
          <Button size="sm" variant="ghost" className={styles.cardBtn} onClick={(e) => { e.stopPropagation(); onOpen() }}>
            View
          </Button>
        ) : (
          <Button size="sm" className={styles.cardBtn} onClick={(e) => { e.stopPropagation(); onJoin(c.id) }}>
            Join
          </Button>
        )}
      </div>
    </div>
  )
}

export function CommunitiesPage({
  communities,
  profile,
  onNavigate,
  onAddFriend,
  onOpenChat
}: {
  communities: CommunitiesState
  profile: ProfileState
  onNavigate: (page: string) => void
  onAddFriend: (address: string) => void
  onOpenChat: () => void
}): React.JSX.Element | null {
  const [query, setQuery] = useState('')
  const [openId, setOpenId] = useState<string | null>(null)
  const { loadDetail } = communities

  // Load the per-community detail (members/posts/places/events) when a modal opens.
  useEffect(() => {
    if (openId != null) loadDetail(openId)
  }, [openId, loadDetail])

  const mine = useMemo(() => communities.list.filter((c) => isMember(c.role)), [communities.list])
  const browse = useMemo(
    () => communities.list.filter((c) => !query || c.name.toLowerCase().includes(query.toLowerCase())),
    [communities.list, query]
  )

  if (!communities.open) return null

  const selected = openId ? communities.list.find((c) => c.id === openId) ?? null : null
  const p = profile.data
  return (
    <MainMenuShell
      active="communities"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={communities.toggle}
    >
      <div className={styles.head}>
        <h1 className={styles.title}>Communities</h1>
        <input
          className={styles.search}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search"
        />
      </div>

      <div className={styles.layout}>
        <aside className={styles.side}>
          <button type="button" className={styles.createBtn}>+ CREATE A COMMUNITY</button>
          <div className={styles.sideSection}>
            <span className={styles.sideHead}>Invites &amp; Requests</span>
          </div>
          <div className={styles.myHead}>
            <span className={styles.sideHead}>My Communities</span>
            {mine.length > 0 && <span className={styles.viewAll}>VIEW ALL ›</span>}
          </div>
          {mine.length === 0 ? (
            <span className={styles.sideEmpty}>You haven't joined any communities yet.</span>
          ) : (
            mine.map((c) => (
              <div key={c.id} className={styles.sideRow} onClick={() => setOpenId(c.id)} role="button" tabIndex={0}>
                <Avatar src={c.thumbnail} name={c.name} color={nameColor(c.id)} size={36} />
                <span className={styles.sideName} title={c.name}>{c.name}</span>
              </div>
            ))
          )}
        </aside>

        <section className={styles.main}>
          <span className={styles.browseHead}>Browse Communities ({browse.length})</span>
          {browse.length === 0 ? (
            <div className={styles.empty}>No communities found.</div>
          ) : (
            <div className={styles.grid}>
              {browse.map((c) => (
                <CommunityCard key={c.id} c={c} onJoin={communities.join} onOpen={() => setOpenId(c.id)} />
              ))}
            </div>
          )}
        </section>
      </div>

      {selected && (
        <CommunityModal
          community={selected}
          detail={communities.detail != null && communities.detail.id === selected.id ? communities.detail : null}
          onJoin={communities.join}
          onLeave={communities.leave}
          onAddFriend={onAddFriend}
          onOpenChat={() => { setOpenId(null); onOpenChat() }}
          onClose={() => setOpenId(null)}
        />
      )}
    </MainMenuShell>
  )
}
