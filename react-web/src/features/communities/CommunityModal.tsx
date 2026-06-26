// Community detail modal — matches Unity's CommunityCardView: a wide card with a header
// (thumbnail, name, public/private, members, description, open-chat, Join/Joined, ⋮),
// ANNOUNCEMENTS / MEMBERS / PLACES / PHOTOS tabs, and an Upcoming Events sidebar. Data
// (members/posts/places/events) arrives via the bridge `communityDetail` relay.

import { useState } from 'react'
import { Avatar, Button } from '../../design'
import { nameColor } from '../../lib/identity'
import type {
  Community,
  CommunityDetailMessage,
  CommunityEvent,
  CommunityMember,
  CommunityPhoto,
  CommunityPlace,
  CommunityPost
} from '../../engine/protocol'
import styles from './CommunityModal.module.css'

type Tab = 'announcements' | 'members' | 'places' | 'photos'
const TABS: { id: Tab; label: string }[] = [
  { id: 'announcements', label: 'ANNOUNCEMENTS' },
  { id: 'members', label: 'MEMBERS' },
  { id: 'places', label: 'PLACES' },
  { id: 'photos', label: 'PHOTOS' }
]

function isMemberRole(role: string): boolean {
  return role === 'owner' || role === 'moderator' || role === 'member'
}
function compact(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k`
  return String(n)
}
function roleLabel(role: string): string | null {
  if (role === 'owner') return 'Owner'
  if (role === 'moderator') return 'Moderator'
  return null
}
function postDate(ts: number): string {
  return new Date(ts).toLocaleDateString([], { month: 'short', day: 'numeric' })
}
function eventDate(ts: number): string {
  const d = new Date(ts)
  const wd = d.toLocaleDateString([], { weekday: 'short' }).toUpperCase()
  const md = d.toLocaleDateString([], { month: 'short', day: 'numeric' }).toUpperCase()
  const t = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
  return `${wd}, ${md} @ ${t}`
}

function GlobeIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.7" />
      <path d="M3 12h18M12 3c2.5 2.5 2.5 15 0 18M12 3c-2.5 2.5-2.5 15 0 18" stroke="currentColor" strokeWidth="1.7" />
    </svg>
  )
}
function LockIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="13" height="13" fill="none" aria-hidden="true">
      <rect x="5" y="11" width="14" height="9" rx="2" stroke="currentColor" strokeWidth="1.8" />
      <path d="M8 11V8a4 4 0 0 1 8 0v3" stroke="currentColor" strokeWidth="1.8" />
    </svg>
  )
}
function ChatIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" aria-hidden="true">
      <path d="M20 2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h4l4 4 4-4h4c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
    </svg>
  )
}
function HeartIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" aria-hidden="true">
      <path d="M12 20s-7-4.5-9.5-9C1 8 2.5 4.5 6 4.5c2 0 3.2 1.2 4 2.5.8-1.3 2-2.5 4-2.5 3.5 0 5 3.5 3.5 6.5C19 15.5 12 20 12 20z" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
    </svg>
  )
}
function AddFriendIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" aria-hidden="true">
      <circle cx="9" cy="8" r="3.2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M3.5 19c0-3 2.5-4.6 5.5-4.6s5.5 1.6 5.5 4.6" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
      <path d="M19 8v6M16 11h6" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  )
}
function PinIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="13" height="13" fill="none" aria-hidden="true">
      <path d="M12 21s7-5.7 7-11a7 7 0 1 0-14 0c0 5.3 7 11 7 11z" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
      <circle cx="12" cy="10" r="2.4" stroke="currentColor" strokeWidth="1.7" />
    </svg>
  )
}
function Verified(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true">
      <path d="M12 2l2.4 1.8 3-.3 1 2.8 2.6 1.5-.9 2.9.9 2.9-2.6 1.5-1 2.8-3-.3L12 22l-2.4-1.8-3 .3-1-2.8L3 16.2l.9-2.9L3 10.4l2.6-1.5 1-2.8 3 .3L12 2z" fill="#4d8dff" />
      <path d="M8.5 12l2.2 2.2 4.5-4.5" stroke="#fff" strokeWidth="1.8" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function PostRow({ post }: { post: CommunityPost }): React.JSX.Element {
  return (
    <div className={styles.post}>
      <Avatar src={post.authorPicture} name={post.author} color={nameColor(post.authorAddress)} size={36} />
      <div className={styles.postBody}>
        <div className={styles.postHead}>
          <span className={styles.postAuthor}>{post.author}</span>
          <span className={styles.postDate}>· {postDate(post.timestamp)}</span>
          <span className={styles.postLikes}><HeartIcon /> {post.likes}</span>
        </div>
        <p className={styles.postText}>{post.text}</p>
      </div>
    </div>
  )
}

function MemberRow({ member, requested, onAdd }: { member: CommunityMember; requested: boolean; onAdd: () => void }): React.JSX.Element {
  const label = roleLabel(member.role)
  return (
    <div className={styles.member}>
      <Avatar src={member.picture} name={member.name} color={nameColor(member.address)} size={40} status="online" />
      <div className={styles.memberInfo}>
        <span className={styles.memberName} style={{ color: nameColor(member.address) }}>
          {member.name}
          {member.hasClaimedName && <Verified />}
        </span>
        {label && <span className={styles.memberRole}>{label}</span>}
      </div>
      {!member.isFriend && (
        requested ? (
          <span className={styles.requested}>Requested</span>
        ) : (
          <button type="button" className={styles.addFriend} onClick={onAdd}>
            <AddFriendIcon /> ADD FRIEND
          </button>
        )
      )}
    </div>
  )
}

function PlaceCard({ place }: { place: CommunityPlace }): React.JSX.Element {
  return (
    <div className={styles.place}>
      <div className={styles.placeCover} style={{ background: place.thumbnail ? undefined : nameColor(place.id) }}>
        {place.thumbnail && <img src={place.thumbnail} alt="" />}
      </div>
      <span className={styles.placeName} title={place.title}>{place.title}</span>
      <div className={styles.placeMeta}>
        {place.likeRate != null && <span className={styles.placeLike}>👍 {Math.round(place.likeRate * 100)}%</span>}
        {place.positions && <span className={styles.placePos}><PinIcon /> {place.positions}</span>}
      </div>
    </div>
  )
}

function PhotoTile({ photo }: { photo: CommunityPhoto }): React.JSX.Element {
  return (
    <a className={styles.photo} href={photo.url} target="_blank" rel="noreferrer">
      <img src={photo.thumbnail ?? photo.url} alt="" loading="lazy" />
    </a>
  )
}

function EventRow({ event }: { event: CommunityEvent }): React.JSX.Element {
  return (
    <div className={styles.event}>
      <div className={styles.eventThumb} style={{ background: event.thumbnail ? undefined : nameColor(event.id) }}>
        {event.thumbnail && <img src={event.thumbnail} alt="" />}
      </div>
      <div className={styles.eventInfo}>
        <span className={styles.eventDate}>{eventDate(event.startsAt)}</span>
        <span className={styles.eventName} title={event.name}>{event.name}</span>
      </div>
    </div>
  )
}

export function CommunityModal({
  community,
  detail,
  onJoin,
  onLeave,
  onAddFriend,
  onOpenChat,
  onClose
}: {
  community: Community
  detail: CommunityDetailMessage | null
  onJoin: (id: string) => void
  onLeave: (id: string) => void
  onAddFriend: (address: string) => void
  onOpenChat: () => void
  onClose: () => void
}): React.JSX.Element {
  const [tab, setTab] = useState<Tab>('announcements')
  const [coverFailed, setCoverFailed] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [requested, setRequested] = useState<ReadonlySet<string>>(new Set())
  const member = isMemberRole(community.role)
  const isPrivate = community.privacy === 'private'
  const showCover = community.thumbnail != null && !coverFailed
  const loading = detail == null

  const addFriend = (address: string): void => {
    setRequested((prev) => new Set(prev).add(address))
    onAddFriend(address)
  }
  const copyLink = (): void => {
    void navigator.clipboard?.writeText(`https://decentraland.org/communities/${community.id}`).catch(() => undefined)
    setMenuOpen(false)
  }

  return (
    <div className={styles.scrim} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.main}>
          <header className={styles.header}>
            <div className={styles.thumb} style={{ background: showCover ? undefined : nameColor(community.id) }}>
              {showCover ? (
                <img src={community.thumbnail} alt="" onError={() => setCoverFailed(true)} />
              ) : (
                <span>{community.name.charAt(0).toUpperCase()}</span>
              )}
            </div>
            <div className={styles.headInfo}>
              <h2 className={styles.name} title={community.name}>{community.name}</h2>
              <div className={styles.meta}>
                <span className={styles.privacy}>{isPrivate ? <><LockIcon /> Private</> : <><GlobeIcon /> Public</>}</span>
                <span className={styles.bar}>|</span>
                <span><b>{compact(community.membersCount)}</b> Members</span>
              </div>
              {community.description && <p className={styles.desc}>{community.description}</p>}
            </div>
            <div className={styles.headActions}>
              {member && (
                <button type="button" className={styles.iconBtn} aria-label="Open chat" onClick={onOpenChat}><ChatIcon /></button>
              )}
              {member ? (
                <Button size="sm" variant="ghost" className={styles.joined} disabled>✓ Joined</Button>
              ) : (
                <Button size="sm" onClick={() => onJoin(community.id)}>{isPrivate ? 'Request to Join' : 'Join'}</Button>
              )}
              <div className={styles.kebabWrap}>
                <button type="button" className={styles.kebab} aria-label="More" onClick={() => setMenuOpen((o) => !o)}>⋮</button>
                {menuOpen && (
                  <div className={styles.menu}>
                    <button type="button" className={styles.menuItem} onClick={copyLink}>Copy link</button>
                    {member && (
                      <button type="button" className={`${styles.menuItem} ${styles.menuDanger}`} onClick={() => { setMenuOpen(false); onLeave(community.id); onClose() }}>
                        Leave community
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          </header>

          <nav className={styles.tabs}>
            {TABS.map((t) => (
              <button
                key={t.id}
                type="button"
                className={`${styles.tab} ${tab === t.id ? styles.tabActive : ''}`.trim()}
                onClick={() => setTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </nav>

          <div className={styles.tabBody}>
            {loading ? (
              <div className={styles.empty}>Loading…</div>
            ) : tab === 'announcements' ? (
              detail.posts.length > 0 ? detail.posts.map((p) => <PostRow key={p.id} post={p} />) : <div className={styles.empty}>No announcements yet.</div>
            ) : tab === 'members' ? (
              detail.members.length > 0 ? (
                <div className={styles.memberGrid}>{detail.members.map((m) => <MemberRow key={m.address} member={m} requested={requested.has(m.address)} onAdd={() => addFriend(m.address)} />)}</div>
              ) : <div className={styles.empty}>No members to show.</div>
            ) : tab === 'places' ? (
              detail.places.length > 0 ? (
                <div className={styles.placeGrid}>{detail.places.map((p) => <PlaceCard key={p.id} place={p} />)}</div>
              ) : <div className={styles.empty}>No places shared yet.</div>
            ) : detail.photos.length > 0 ? (
              <div className={styles.photoGrid}>{detail.photos.map((ph) => <PhotoTile key={ph.id} photo={ph} />)}</div>
            ) : (
              <div className={styles.empty}>No photos shared yet.</div>
            )}
          </div>
        </div>

        <aside className={styles.events}>
          <div className={styles.eventsHead}>Upcoming Events</div>
          <div className={styles.eventsList}>
            {loading ? (
              <div className={styles.empty}>Loading…</div>
            ) : detail.events.length > 0 ? (
              detail.events.map((e) => <EventRow key={e.id} event={e} />)
            ) : (
              <div className={styles.empty}>No upcoming events.</div>
            )}
          </div>
        </aside>

        <button type="button" className={styles.close} aria-label="Close" onClick={onClose}>×</button>
      </div>
    </div>
  )
}
