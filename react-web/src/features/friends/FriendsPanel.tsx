// React friends panel — Explorer 2.0 design. Tabs: Friends / Requests / Blocked.
// Friends are grouped Online/Offline (collapsible); requests have Accept/Delete
// actions; blocked shows an empty placeholder. Data + actions come from the bridge
// relay of the scene social state (BevyApi.social.*), guest-disabled.

import { useMemo, useState } from 'react'
import { Avatar, Button, ControlButton } from '../../design'
import { nameColor, shortAddr, splitName } from '../../lib/identity'
import type { Friend, FriendRequest } from '../../engine/protocol'
import type { FriendsState } from '../session/useEngineSession'
import { ProfileCard, type ChatUser, type Relationship } from '../chat/ProfileCard'
import styles from './FriendsPanel.module.css'

type Tab = 'friends' | 'requests' | 'blocked'
type OpenMenu = (user: ChatUser, e: React.MouseEvent) => void

function label(name: string, address: string): string {
  return name.trim() ? name : shortAddr(address)
}

/** Claimed names (no #suffix, not a raw address) get the verified check. */
function isClaimed(name: string): boolean {
  return name.trim().length > 0 && !name.includes('#') && !/^0x[0-9a-f]+$/i.test(name)
}

function Verified(): React.JSX.Element {
  return (
    <svg className={styles.verified} viewBox="0 0 16 16" aria-label="verified">
      <defs>
        <linearGradient id="vrf" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ff2d55" />
          <stop offset="1" stopColor="#c640cd" />
        </linearGradient>
      </defs>
      <path
        d="M8 1l1.7 1.2 2.1-.2 1 1.8 1.9.9-.5 2 .9 1.9-1.6 1.4.1 2.1-2 .6-1.1 1.8-2-.7-2 .7-1.1-1.8-2-.6.1-2.1L1.6 8.6l.9-1.9-.5-2 1.9-.9 1-1.8 2.1.2z"
        fill="url(#vrf)"
      />
      <path d="M5.5 8l1.7 1.7L10.8 6" stroke="#fff" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function NameLabel({ name, address }: { name: string; address: string }): React.JSX.Element {
  const { base, tag } = splitName(label(name, address))
  return (
    <span className={styles.name} style={{ color: nameColor(address) }}>
      {base}
      {isClaimed(name) && <Verified />}
      {tag && <span className={styles.tag}>{tag}</span>}
    </span>
  )
}

function Chevron({ open }: { open: boolean }): React.JSX.Element {
  return (
    <svg className={`${styles.chev} ${open ? styles.chevOpen : ''}`.trim()} viewBox="0 0 12 12" aria-hidden="true">
      <path d="M2.5 7.5L6 4l3.5 3.5" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function Collapsible({
  title,
  count,
  children
}: {
  title: string
  count: number
  children: React.ReactNode
}): React.JSX.Element {
  const [open, setOpen] = useState(true)
  return (
    <>
      <button type="button" className={styles.sectionHead} onClick={() => setOpen((o) => !o)}>
        <Chevron open={open} />
        {title} ({count})
      </button>
      {open && children}
    </>
  )
}

const STATUS_LABEL = { online: 'Online', away: 'Away', offline: 'Offline' } as const

function FriendRow({ friend, onOpen }: { friend: Friend; onOpen?: OpenMenu }): React.JSX.Element {
  const user: ChatUser = { address: friend.address, name: friend.name, picture: friend.picture }
  const open = (e: React.MouseEvent): void => {
    if (e.type === 'contextmenu') e.preventDefault()
    onOpen?.(user, e)
  }
  return (
    <button type="button" className={`${styles.row} ${styles.rowBtn}`} onClick={open} onContextMenu={open}>
      <Avatar src={friend.picture} name={label(friend.name, friend.address)} color={nameColor(friend.address)} size={40} status={friend.status} />
      <div className={styles.info}>
        <NameLabel name={friend.name} address={friend.address} />
        <span className={`${styles.status} ${styles[friend.status]}`}>{STATUS_LABEL[friend.status]}</span>
      </div>
    </button>
  )
}

function reqDate(ts?: number): string {
  if (!ts) return ''
  return new Date(ts).toLocaleDateString([], { month: 'short', day: 'numeric' }).toUpperCase()
}

function ReceivedRow({
  req,
  onAccept,
  onReject
}: {
  req: FriendRequest
  onAccept: () => void
  onReject: () => void
}): React.JSX.Element {
  return (
    <div className={styles.row}>
      <Avatar src={req.picture} name={label(req.name, req.address)} color={nameColor(req.address)} size={40} />
      <div className={styles.info}>
        <NameLabel name={req.name} address={req.address} />
        {req.message && <span className={styles.sub}>{req.message}</span>}
      </div>
      <span className={styles.date}>{reqDate(req.createdAt)}</span>
      <div className={styles.actions}>
        <Button variant="secondary" size="sm" onClick={onReject}>
          Delete
        </Button>
        <Button variant="primary" size="sm" onClick={onAccept}>
          Accept
        </Button>
      </div>
    </div>
  )
}

function SentRow({ req, onCancel }: { req: FriendRequest; onCancel: () => void }): React.JSX.Element {
  return (
    <div className={styles.row}>
      <Avatar src={req.picture} name={label(req.name, req.address)} color={nameColor(req.address)} size={40} />
      <div className={styles.info}>
        <NameLabel name={req.name} address={req.address} />
        <span className={styles.sub}>Request sent</span>
      </div>
      <span className={styles.date}>{reqDate(req.createdAt)}</span>
      <div className={styles.actions}>
        <Button variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  )
}

export function FriendsPanel({
  friends,
  me,
  relationshipOf,
  onViewProfile,
  onReport,
  onMention
}: {
  friends: FriendsState
  me?: { address?: string } | null
  /** Friendship status for a user (drives the profile card CTA). */
  relationshipOf?: (address: string) => Relationship
  onViewProfile?: (user: ChatUser) => void
  onReport?: (user: ChatUser) => void
  onMention?: (name: string) => void
}): React.JSX.Element | null {
  const [tab, setTab] = useState<Tab>('friends')
  // Same profile menu the chat opens, anchored at the clicked row.
  const [menu, setMenu] = useState<{ user: ChatUser; x: number; y: number } | null>(null)
  const openMenu: OpenMenu = (user, e) => setMenu({ user, x: e.clientX, y: e.clientY })

  const { online, offline } = useMemo(() => {
    const on: Friend[] = []
    const off: Friend[] = []
    for (const f of [...friends.list].sort((a, b) => a.name.localeCompare(b.name))) {
      ;(f.status === 'offline' ? off : on).push(f)
    }
    return { online: on, offline: off }
  }, [friends.list])

  if (!friends.open) return null

  const requestCount = friends.received.length
  const TABS: { id: Tab; label: string; badge?: number }[] = [
    { id: 'friends', label: 'Friends' },
    { id: 'requests', label: 'Requests', badge: requestCount },
    { id: 'blocked', label: 'Blocked' }
  ]

  return (
    <div className={styles.root}>
      <header className={styles.tabs}>
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`${styles.tab} ${tab === t.id ? styles.tabActive : ''}`.trim()}
            onClick={() => setTab(t.id)}
          >
            {t.label}
            {t.badge ? <span className={styles.tabBadge}>{t.badge}</span> : null}
          </button>
        ))}
        <ControlButton variant="solid" className={styles.closeGlyph} aria-label="Close friends" onClick={friends.toggle}>
          ×
        </ControlButton>
      </header>

      <div className={styles.body}>
        {!friends.available ? (
          <div className={styles.placeholder}>
            <div className={styles.phTitle}>Friends aren’t available</div>
            <div className={styles.phText}>Sign in with a wallet to add and manage friends.</div>
          </div>
        ) : tab === 'friends' ? (
          friends.list.length === 0 ? (
            <div className={styles.empty}>No friends yet.</div>
          ) : (
            <>
              <Collapsible title="Online" count={online.length}>
                {online.map((f) => (
                  <FriendRow key={f.address} friend={f} onOpen={openMenu} />
                ))}
              </Collapsible>
              <Collapsible title="Offline" count={offline.length}>
                {offline.map((f) => (
                  <FriendRow key={f.address} friend={f} onOpen={openMenu} />
                ))}
              </Collapsible>
            </>
          )
        ) : tab === 'requests' ? (
          <>
            <Collapsible title="Received" count={friends.received.length}>
              {friends.received.length === 0 ? (
                <div className={styles.empty}>No Requests</div>
              ) : (
                friends.received.map((r) => (
                  <ReceivedRow
                    key={r.id}
                    req={r}
                    onAccept={() => friends.act('accept', r.address)}
                    onReject={() => friends.act('reject', r.address)}
                  />
                ))
              )}
            </Collapsible>
            <Collapsible title="Sent" count={friends.sent.length}>
              {friends.sent.length === 0 ? (
                <div className={styles.empty}>No Requests</div>
              ) : (
                friends.sent.map((r) => (
                  <SentRow key={r.id} req={r} onCancel={() => friends.act('cancel', r.address)} />
                ))
              )}
            </Collapsible>
          </>
        ) : friends.blocked.length === 0 ? (
          <div className={styles.placeholder}>
            <div className={styles.phIcon} aria-hidden="true">
              ⊘
            </div>
            <div className={styles.phTitle}>No Blocked Accounts</div>
            <div className={styles.phText}>
              If you block someone, you won’t see each other in-world or exchange messages, and their
              name and messages are hidden in public chats.
            </div>
          </div>
        ) : (
          friends.blocked.map((addr) => (
            <div key={addr} className={styles.row}>
              <Avatar name={shortAddr(addr)} color={nameColor(addr)} size={40} />
              <div className={styles.info}>
                <span className={styles.name}>{shortAddr(addr)}</span>
              </div>
              <div className={styles.actions}>
                <Button variant="secondary" size="sm" onClick={() => friends.act('unblock', addr)}>
                  Unblock
                </Button>
              </div>
            </div>
          ))
        )}
      </div>

      {menu && (
        <ProfileCard
          user={menu.user}
          x={menu.x}
          y={menu.y}
          me={me}
          relationship={relationshipOf?.(menu.user.address) ?? 'friend'}
          onFriendAction={friends.act}
          onViewProfile={onViewProfile}
          onMention={onMention}
          onReport={onReport}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  )
}
