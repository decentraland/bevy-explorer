// React friends panel — Explorer 2.0 design. Tabs: Friends / Requests / Blocked.
// Friends are grouped Online/Offline (collapsible); requests have Accept/Delete
// actions; blocked shows an empty placeholder. Data + actions come from the bridge
// relay of the scene social state (BevyApi.social.*), guest-disabled.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Avatar, BlockedUser, Button, ControlButton, Envelope, Kebab, Tabs, Tooltip, hasOpenPopup, type TabItem } from '../../design'
import { nameColor, shortAddr, splitName } from '../../lib/identity'
import type { Friend, FriendRequest } from '../../engine/protocol'
import type { FriendsState } from '../session/useEngineSession'
import { type ChatUser } from '../chat/ProfileCardPresentation'
import { openProfileCard } from '../profileCard/ProfileCard'
import { useProfile } from '../session/profileStore'
import { hudInsetRef } from '../../lib/hudInset'
import styles from './FriendsPanel.module.css'

type Tab = 'friends' | 'requests' | 'blocked'
type OpenMenu = (user: ChatUser, e: React.MouseEvent) => void
type MenuAt = (address: string, x: number, y: number) => void

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

function NameLabel({ name, address, message }: { name: string; address: string; message?: boolean }): React.JSX.Element {
  const { base, tag } = splitName(label(name, address))
  return (
    <span className={styles.name} style={{ color: nameColor(address) }}>
      {base}
      {tag && <span className={styles.tag}>{tag}</span>}
      {isClaimed(name) && <Verified />}
      {message && <Envelope className={styles.envelope} />}
    </span>
  )
}

/** The row's ⋮ button: opens the user menu anchored under it. */
function RowMenu({ address, onOpen }: { address: string; onOpen: (address: string, x: number, y: number) => void }): React.JSX.Element {
  return (
    <Tooltip label="Menu" side="top" variant="rail">
      <button
        type="button"
        className={styles.menuBtn}
        aria-label="More options"
        onClick={(e) => {
          e.stopPropagation()
          const r = e.currentTarget.getBoundingClientRect()
          onOpen(address, r.left, r.bottom)
        }}
      >
        <Kebab vertical size={20} r={2} />
      </button>
    </Tooltip>
  )
}

function Chevron({ open }: { open: boolean }): React.JSX.Element {
  return (
    <svg className={open ? styles.chev : `${styles.chev} ${styles.chevClosed}`} viewBox="0 0 12 7" aria-hidden="true">
      <path d="M1 6L6 1l5 5" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function Collapsible({
  title,
  count,
  emptyLabel,
  open,
  onToggle,
  children
}: {
  title: string
  count: number
  emptyLabel: string
  open: boolean
  onToggle: () => void
  children: React.ReactNode
}): React.JSX.Element {
  return (
    <>
      <button type="button" className={styles.sectionHead} aria-expanded={open} onClick={onToggle}>
        <Chevron open={open} />
        {title} ({count})
      </button>
      {open && (count === 0 ? <div className={styles.empty}>{emptyLabel}</div> : children)}
    </>
  )
}

const STATUS_LABEL = { online: 'Online', away: 'Away', offline: 'Offline' } as const

/** Who a row shows: the profile store's copy once it has one (the engine's, kept current), and
 *  until then the name and face the friends service sent — which only ever seed the store, so a
 *  service that stops sending faces costs one round trip, not the face. */
function useRowIdentity(user: { address: string; name: string; picture?: string }): { name: string; picture?: string } {
  const known = useProfile(user.address)
  return { name: known?.name ?? user.name, picture: known?.picture ?? user.picture }
}

function FriendRow({ friend, onOpen, onMenu }: { friend: Friend; onOpen?: OpenMenu; onMenu: MenuAt }): React.JSX.Element {
  const { name, picture } = useRowIdentity(friend)
  const user: ChatUser = { address: friend.address, name, picture }
  const open = (e: React.MouseEvent): void => {
    if (e.type === 'contextmenu') e.preventDefault()
    onOpen?.(user, e)
  }
  return (
    <div
      role="button"
      tabIndex={0}
      className={`${styles.row} ${styles.rowBtn}`}
      onClick={open}
      onContextMenu={open}
      onKeyDown={(e) => {
        if (e.key !== 'Enter' && e.key !== ' ') return
        e.preventDefault()
        const r = e.currentTarget.getBoundingClientRect()
        onMenu(friend.address, r.left, r.bottom)
      }}
    >
      <Avatar src={picture} name={label(name, friend.address)} color={nameColor(friend.address)} size={40} status={friend.status} dotPosition="top" />
      <div className={styles.info}>
        <NameLabel name={name} address={friend.address} />
        <span className={styles.status}>{STATUS_LABEL[friend.status]}</span>
      </div>
      <div className={styles.hoverActions}>
        <RowMenu address={friend.address} onOpen={onMenu} />
      </div>
    </div>
  )
}

const MONTHS = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC']

function reqDate(ts?: number): string {
  if (!ts) return ''
  const d = new Date(ts)
  return `${MONTHS[d.getMonth()]} ${String(d.getDate()).padStart(2, '0')}`
}

function ReceivedRow({
  req,
  onAccept,
  onReject,
  onMenu
}: {
  req: FriendRequest
  onAccept: () => void
  onReject: () => void
  onMenu: MenuAt
}): React.JSX.Element {
  const { name, picture } = useRowIdentity(req)
  return (
    <div className={styles.row}>
      <Avatar src={picture} name={label(name, req.address)} color={nameColor(req.address)} size={40} />
      <div className={styles.info}>
        <NameLabel name={name} address={req.address} message={Boolean(req.message)} />
      </div>
      <span className={styles.date}>{reqDate(req.createdAt)}</span>
      <div className={styles.actions}>
        <Button variant="secondary" size="row" onClick={onReject}>
          Delete
        </Button>
        <Button variant="primary" size="row" onClick={onAccept}>
          Accept
        </Button>
        <RowMenu address={req.address} onOpen={onMenu} />
      </div>
    </div>
  )
}

function SentRow({ req, onCancel, onMenu }: { req: FriendRequest; onCancel: () => void; onMenu: MenuAt }): React.JSX.Element {
  const { name, picture } = useRowIdentity(req)
  return (
    <div className={styles.row}>
      <Avatar src={picture} name={label(name, req.address)} color={nameColor(req.address)} size={40} />
      <div className={styles.info}>
        <NameLabel name={name} address={req.address} />
      </div>
      <span className={styles.date}>{reqDate(req.createdAt)}</span>
      <div className={styles.actions}>
        <Button variant="secondary" size="row" onClick={onCancel}>
          Cancel
        </Button>
        <RowMenu address={req.address} onOpen={onMenu} />
      </div>
    </div>
  )
}

export function FriendsPanel({
  friends
}: {
  friends: FriendsState
}): React.JSX.Element | null {
  const [tab, setTab] = useState<Tab>('friends')
  // Kept above the closed early-return so folded sections stay folded across reopen.
  const [folded, setFolded] = useState<Record<string, boolean>>({})
  const fold = (id: string) => ({ open: !folded[id], onToggle: () => setFolded((f) => ({ ...f, [id]: !f[id] })) })
  const rootRef = useRef<HTMLDivElement | null>(null)
  const setRoot = useCallback((el: HTMLDivElement | null) => {
    rootRef.current = el
    const cleanup = hudInsetRef(el)
    return () => {
      rootRef.current = null
      cleanup?.()
    }
  }, [])

  useEffect(() => {
    if (friends.open) setTab('friends')
  }, [friends.open])

  // Clicking anywhere outside (except the rail's own toggle and popups opened from here) closes it.
  const toggle = friends.toggle
  useEffect(() => {
    if (!friends.open) return
    const onDown = (e: PointerEvent): void => {
      const t = e.target as Node | null
      if (t == null || rootRef.current?.contains(t) || hasOpenPopup()) return
      if (t instanceof Element && t.closest('nav[aria-label="Main navigation"]')) return
      toggle()
    }
    document.addEventListener('pointerdown', onDown, true)
    return () => document.removeEventListener('pointerdown', onDown, true)
  }, [friends.open, toggle])
  // Row click opens the shared profile card at the click (same card the chat + world open).
  const openMenu: OpenMenu = (user, e) => openProfileCard(user.address, e.clientX, e.clientY)
  const menuAt: MenuAt = (address, x, y) => openProfileCard(address, x, y)

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
  const TABS: TabItem<Tab>[] = [
    { id: 'friends', label: 'Friends' },
    { id: 'requests', label: 'Requests', badge: requestCount, badgeMax: 9 },
    { id: 'blocked', label: 'Blocked' }
  ]

  return (
    <div ref={setRoot} className={styles.root}>
      <header className={styles.head}>
        <Tabs variant="underline" fill className={styles.tabs} items={TABS} value={tab} onChange={setTab} aria-label="Friends sections" />
        <ControlButton variant="solid" size="lg" className={styles.closeGlyph} aria-label="Close friends" onClick={friends.toggle}>
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
            <div className={styles.placeholder}>
              <div className={styles.phTitle}>Time To Make Some Friends!</div>
              <div className={styles.phText}>
                View someone’s Profile or click on their name in the Chat to see the <b>‘Add Friend’</b> option.
              </div>
            </div>
          ) : (
            <>
              <Collapsible title="Online" {...fold('online')} count={online.length} emptyLabel="No Friends">
                {online.map((f) => (
                  <FriendRow key={f.address} friend={f} onOpen={openMenu} onMenu={menuAt} />
                ))}
              </Collapsible>
              <Collapsible title="Offline" {...fold('offline')} count={offline.length} emptyLabel="No Friends">
                {offline.map((f) => (
                  <FriendRow key={f.address} friend={f} onOpen={openMenu} onMenu={menuAt} />
                ))}
              </Collapsible>
            </>
          )
        ) : tab === 'requests' ? (
          <>
            <Collapsible title="Received" {...fold('received')} count={friends.received.length} emptyLabel="No Requests">
              {friends.received.map((r) => (
                <ReceivedRow
                  key={r.id}
                  req={r}
                  onAccept={() => friends.act('accept', r.address)}
                  onReject={() => friends.act('reject', r.address)}
                  onMenu={menuAt}
                />
              ))}
            </Collapsible>
            <Collapsible title="Sent" {...fold('sent')} count={friends.sent.length} emptyLabel="No Requests">
              {friends.sent.map((r) => (
                <SentRow key={r.id} req={r} onCancel={() => friends.act('cancel', r.address)} onMenu={menuAt} />
              ))}
            </Collapsible>
          </>
        ) : friends.blocked.length === 0 ? (
          <div className={styles.placeholder}>
            <BlockedUser size={72} className={styles.phIcon} />
            <div className={styles.phTitle}>No Blocked Accounts</div>
            <div className={styles.phText}>
              If you block someone, you will not be able to see each other in-world or exchange messages. You will also not see each other’s names or messages
              in public chats.
            </div>
            <div className={styles.phHint}>
              The option to block an account is available in the <Kebab size={16} /> menu on their Profile or when you click on their name in the Chat.
            </div>
          </div>
        ) : (
          friends.blocked.map((addr) => (
            <div key={addr} className={styles.row}>
              <Avatar name={shortAddr(addr)} color={nameColor(addr)} size={40} />
              <div className={styles.info}>
                <span className={styles.name}>{shortAddr(addr)}</span>
              </div>
              <div className={`${styles.actions} ${styles.blockedActions}`}>
                <Button variant="secondary" size="row" className={styles.unblock} onClick={() => friends.act('unblock', addr)}>
                  Unblock
                </Button>
                <RowMenu address={addr} onOpen={menuAt} />
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
