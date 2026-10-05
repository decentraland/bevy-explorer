// React friends panel — Explorer 2.0 design. Tabs: Friends / Requests / Blocked.
// Friends are grouped Online/Offline (collapsible); requests have Accept/Delete
// actions; blocked shows an empty placeholder. Data + actions come from the bridge
// relay of the scene social state (BevyApi.social.*), guest-disabled.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Avatar, BlockedUser, Button, ControlButton, Envelope, Kebab, Spinner, Tabs, Tooltip, VerifiedBadge, hasOpenPopup, type TabItem } from '../../design'
import { shortAddr, splitName, userColor } from '../../lib/identity'
import type { BlockedUser as Blocked, Friend, FriendRequest } from '../../engine/protocol'
import type { FriendsState } from '../session/useEngineSession'
import type { MenuContext } from '../chat/ProfileCardPresentation'
import { openProfileCard } from '../profileCard/ProfileCard'
import { openPassport } from '../profile/Passport'
import { confirmUnblock } from './friendDialogs'
import { openFriendRequest } from './FriendRequestPopup'
import { useProfile } from '../session/profileStore'
import { hudInsetRef } from '../../lib/hudInset'
import { looksClaimed } from '../../engine/mention'
import styles from './FriendsPanel.module.css'

type Tab = 'friends' | 'requests' | 'blocked'
/** Open the user menu for `address` above the element that was clicked. */
type MenuAt = (address: string, el: HTMLElement, context?: MenuContext) => void

function label(name: string, address: string): string {
  return name.trim() ? name : shortAddr(address)
}


interface Identity {
  address: string
  claimed?: boolean
  nameColor?: { r: number; g: number; b: number }
}

function colorOf(user: Identity & { name: string }): string {
  return userColor(user.address, user.name, user.claimed, user.nameColor)
}

function NameLabel({ name, user, message }: { name: string; user: Identity; message?: boolean }): React.JSX.Element {
  const { base, tag } = splitName(label(name, user.address))
  const claimed = user.claimed ?? looksClaimed(name)
  return (
    <span className={styles.name} style={{ color: colorOf({ ...user, name }) }}>
      {base}
      {!claimed && tag && <span className={styles.tag}>{tag}</span>}
      {claimed && <VerifiedBadge size={14} />}
      {message && <Envelope className={styles.envelope} />}
    </span>
  )
}

/** The row's ⋮ button: opens the user menu anchored under it. */
function RowMenu({ address, onOpen }: { address: string; onOpen: MenuAt }): React.JSX.Element {
  return (
    <Tooltip label="Menu" side="top" variant="rail">
      <button
        type="button"
        className={styles.menuBtn}
        aria-label="More options"
        onClick={(e) => {
          e.stopPropagation()
          onOpen(address, e.currentTarget)
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

/** A clickable list row: the row opens its target, the avatar and ⋮ open the user menu, and the
 *  row keeps its hover look while that menu is open. */
function Row({
  address,
  avatar,
  menuOpen,
  onOpen,
  onMenu,
  children
}: {
  address: string
  avatar: React.ReactNode
  menuOpen: boolean
  onOpen: () => void
  onMenu: MenuAt
  children: React.ReactNode
}): React.JSX.Element {
  return (
    <div
      role="button"
      tabIndex={0}
      className={`${styles.row} ${styles.rowBtn} ${menuOpen ? styles.rowMenuOpen : ''}`.trim()}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.target !== e.currentTarget || (e.key !== 'Enter' && e.key !== ' ')) return
        e.preventDefault()
        onOpen()
      }}
    >
      <button
        type="button"
        className={styles.avatarBtn}
        aria-label="Open menu"
        onClick={(e) => {
          e.stopPropagation()
          onMenu(address, e.currentTarget)
        }}
      >
        {avatar}
      </button>
      {children}
    </div>
  )
}

interface RowProps {
  menuOpen: boolean
  onMenu: MenuAt
  pending: ReadonlySet<string>
}
const isPending = (pending: ReadonlySet<string>, op: string, address: string): boolean => pending.has(`${op}:${address.toLowerCase()}`)

function FriendRow({ friend, menuOpen, onMenu }: { friend: Friend } & Omit<RowProps, 'pending'>): React.JSX.Element {
  const { name, picture } = useRowIdentity(friend)
  return (
    <Row
      address={friend.address}
      menuOpen={menuOpen}
      onMenu={(a, el) => onMenu(a, el, 'friend')}
      onOpen={() => openPassport(friend.address)}
      avatar={<Avatar src={picture} name={label(name, friend.address)} color={colorOf(friend)} size={40} status={friend.status} dotPosition="top" />}
    >
      <div className={styles.info}>
        <NameLabel name={name} user={friend} />
        <span className={styles.status}>{STATUS_LABEL[friend.status]}</span>
      </div>
      <div className={styles.hoverActions}>
        <RowMenu address={friend.address} onOpen={(a, el) => onMenu(a, el, 'friend')} />
      </div>
    </Row>
  )
}

const MONTHS = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC']

function reqDate(ts?: number): string {
  if (!ts) return ''
  const d = new Date(ts)
  return `${MONTHS[d.getMonth()]} ${String(d.getDate()).padStart(2, '0')}`
}

function RequestRow({
  req,
  mode,
  menuOpen,
  onMenu,
  pending,
  onAccept,
  onReject,
  onCancel
}: {
  req: FriendRequest
  mode: 'received' | 'sent'
  onAccept?: () => void
  onReject?: () => void
  onCancel?: () => void
} & RowProps): React.JSX.Element {
  const { name, picture } = useRowIdentity(req)
  const stop = (fn?: () => void) => (e: React.MouseEvent): void => {
    e.stopPropagation()
    fn?.()
  }
  return (
    <Row
      address={req.address}
      menuOpen={menuOpen}
      onMenu={(a, el) => onMenu(a, el, 'request')}
      onOpen={() => openFriendRequest(mode, { address: req.address, name, picture, message: req.message, createdAt: req.createdAt })}
      avatar={<Avatar src={picture} name={label(name, req.address)} color={colorOf(req)} size={40} />}
    >
      <div className={styles.info}>
        <NameLabel name={name} user={req} message={mode === 'received' && Boolean(req.message)} />
      </div>
      <span className={styles.date}>{reqDate(req.createdAt)}</span>
      <div className={styles.actions}>
        {mode === 'received' ? (
          <>
            <Button variant="secondary" size="row" disabled={isPending(pending, 'reject', req.address)} onClick={stop(onReject)}>
              Delete
            </Button>
            <Button variant="primary" size="row" disabled={isPending(pending, 'accept', req.address)} onClick={stop(onAccept)}>
              Accept
            </Button>
          </>
        ) : (
          <Button variant="secondary" size="row" disabled={isPending(pending, 'cancel', req.address)} onClick={stop(onCancel)}>
            Cancel
          </Button>
        )}
        <RowMenu address={req.address} onOpen={(a, el) => onMenu(a, el, 'request')} />
      </div>
    </Row>
  )
}

const byName = (a: { name: string }, b: { name: string }): number => {
  const x = a.name.toLowerCase()
  const y = b.name.toLowerCase()
  return x < y ? -1 : x > y ? 1 : 0
}
const newestFirst = (a: FriendRequest, b: FriendRequest): number => (b.createdAt ?? 0) - (a.createdAt ?? 0)

function BlockedRow({ user, menuOpen, onMenu, pending, onUnblock }: { user: Blocked; onUnblock: (name: string) => void } & RowProps): React.JSX.Element {
  const { name, picture } = useRowIdentity(user)
  return (
    <Row
      address={user.address}
      menuOpen={menuOpen}
      onMenu={(a, el) => onMenu(a, el, 'blocked')}
      onOpen={() => openPassport(user.address)}
      avatar={<Avatar src={picture} name={label(name, user.address)} color={colorOf(user)} size={40} />}
    >
      <div className={styles.info}>
        <NameLabel name={name} user={user} />
      </div>
      <div className={`${styles.actions} ${styles.blockedActions}`}>
        <Button
          variant="secondary"
          size="row"
          className={styles.unblock}
          disabled={isPending(pending, 'unblock', user.address)}
          onClick={(e) => {
            e.stopPropagation()
            onUnblock(splitName(label(name, user.address)).base)
          }}
        >
          Unblock
        </Button>
        <RowMenu address={user.address} onOpen={(a, el) => onMenu(a, el, 'blocked')} />
      </div>
    </Row>
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
  // The menu opens up from the clicked button (its bottom-left 5px right of and 10px above the
  // button's centre); the row stays highlighted until it closes.
  const [menuFor, setMenuFor] = useState<string | null>(null)
  const menuAt: MenuAt = (address, el, context) => {
    const r = el.getBoundingClientRect()
    setMenuFor(address)
    openProfileCard(address, r.left + r.width / 2 + 5, r.top + r.height / 2 - 10, { above: true, context, onClose: () => setMenuFor(null) })
  }
  const rowProps = (address: string): RowProps => ({ menuOpen: menuFor === address, onMenu: menuAt, pending: friends.pending })

  const { online, offline } = useMemo(() => {
    const on: Friend[] = []
    const off: Friend[] = []
    for (const f of [...friends.list].sort(byName)) {
      ;(f.status === 'offline' ? off : on).push(f)
    }
    return { online: on, offline: off }
  }, [friends.list])
  const received = useMemo(() => [...friends.received].sort(newestFirst), [friends.received])
  const sent = useMemo(() => [...friends.sent].sort(newestFirst), [friends.sent])
  const blocked = useMemo<Blocked[]>(
    () =>
      friends.blockedUsers.length > 0
        ? [...friends.blockedUsers].sort(byName)
        : friends.blocked.map((address) => ({ address, name: '' })),
    [friends.blockedUsers, friends.blocked]
  )

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
        {friends.loading ? (
          <div className={styles.placeholder}>
            <Spinner />
          </div>
        ) : !friends.available ? (
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
                  <FriendRow key={f.address} friend={f} {...rowProps(f.address)} />
                ))}
              </Collapsible>
              <Collapsible title="Offline" {...fold('offline')} count={offline.length} emptyLabel="No Friends">
                {offline.map((f) => (
                  <FriendRow key={f.address} friend={f} {...rowProps(f.address)} />
                ))}
              </Collapsible>
            </>
          )
        ) : tab === 'requests' ? (
          <>
            <Collapsible title="Received" {...fold('received')} count={friends.received.length} emptyLabel="No Requests">
              {received.map((r) => (
                <RequestRow
                  key={r.id}
                  req={r}
                  mode="received"
                  {...rowProps(r.address)}
                  onAccept={() => openFriendRequest('accept', { address: r.address, name: r.name, picture: r.picture })}
                  onReject={() => friends.act('reject', r.address)}
                />
              ))}
            </Collapsible>
            <Collapsible title="Sent" {...fold('sent')} count={friends.sent.length} emptyLabel="No Requests">
              {sent.map((r) => (
                <RequestRow
                  key={r.id}
                  req={r}
                  mode="sent"
                  {...rowProps(r.address)}
                  onCancel={() => friends.act('cancel', r.address)}
                />
              ))}
            </Collapsible>
          </>
        ) : blocked.length === 0 ? (
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
          blocked.map((b) => (
            <BlockedRow
              key={b.address}
              user={b}
              {...rowProps(b.address)}
              onUnblock={(name) => void confirmUnblock(name).then((ok) => ok && friends.act('unblock', b.address))}
            />
          ))
        )}
      </div>
    </div>
  )
}
