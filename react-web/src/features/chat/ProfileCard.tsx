// Profile card (the old SDK7 "profile menu") — the popover shown when you click a
// sender's name/avatar, an @mention, or a nearby avatar in the world. Header with
// avatar / name+copy / address+copy, a relationship-driven friend CTA (Add / Accept +
// Reject / Requested), then the action list — mirroring bevy-ui-scene's profile-menu:
// View Passport · Mention · Invite to Community · Block/Unblock · Report.

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Avatar, ModalShell, Button } from '../../design'
import { nameColor, shortAddr, splitName } from '../../lib/identity'
import type { FriendAction, InvitableCommunity } from '../../engine/protocol'
import styles from './ProfileCard.module.css'

export interface ChatUser {
  address: string
  name: string
  picture?: string
}

/** Relationship of the local user to this profile — drives the friend CTA. */
export type Relationship = 'none' | 'requested' | 'incoming' | 'friend' | 'blocked'

function Verified(): React.JSX.Element {
  return (
    <svg className={styles.verified} viewBox="0 0 16 16" aria-label="verified">
      <defs>
        <linearGradient id="pmv" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ff2d55" />
          <stop offset="1" stopColor="#c640cd" />
        </linearGradient>
      </defs>
      <path d="M8 1l1.7 1.2 2.1-.2 1 1.8 1.9.9-.5 2 .9 1.9-1.6 1.4.1 2.1-2 .6-1.1 1.8-2-.7-2 .7-1.1-1.8-2-.6.1-2.1L1.6 8.6l.9-1.9-.5-2 1.9-.9 1-1.8 2.1.2z" fill="url(#pmv)" />
      <path d="M5.5 8l1.7 1.7L10.8 6" stroke="#fff" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}
function CopyIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  )
}
function AddFriendIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" aria-hidden="true">
      <circle cx="9" cy="8" r="3.4" stroke="currentColor" strokeWidth="1.9" />
      <path d="M3.5 19c0-3.1 2.5-4.8 5.5-4.8" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" />
      <path d="M18 8v6M15 11h6" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" />
    </svg>
  )
}
function ViewProfileIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <rect x="3" y="5" width="18" height="14" rx="2.5" stroke="currentColor" strokeWidth="1.8" />
      <circle cx="8.5" cy="12" r="2.2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M13.5 10.5h4M13.5 14h2.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  )
}
function MentionIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="4" stroke="currentColor" strokeWidth="1.8" />
      <path d="M16 8.5V13a2.5 2.5 0 0 0 5 0V12a9 9 0 1 0-3.6 7.2" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  )
}
function BlockIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <circle cx="10" cy="8" r="3.4" stroke="currentColor" strokeWidth="1.8" />
      <path d="M4 19c0-3.2 2.7-5 6-5 1 0 2 .2 2.8.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
      <circle cx="17.5" cy="16.5" r="4.2" stroke="currentColor" strokeWidth="1.8" />
      <path d="M14.7 13.7l5.6 5.6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  )
}

function CommunityIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <circle cx="8" cy="9" r="2.6" stroke="currentColor" strokeWidth="1.7" />
      <circle cx="16" cy="9" r="2.6" stroke="currentColor" strokeWidth="1.7" />
      <path d="M3.5 18c0-2.5 2-3.8 4.5-3.8s4.5 1.3 4.5 3.8M12.5 14.6c.9-.3 2-.4 3-.4 2.5 0 4 1.3 4 3.8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  )
}
function ReportIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <path d="M12 3l9 16H3L12 3z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
      <path d="M12 10v4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
      <circle cx="12" cy="16.6" r="0.5" fill="currentColor" stroke="currentColor" />
    </svg>
  )
}
function Chevron({ open }: { open: boolean }): React.JSX.Element {
  return (
    <svg className={styles.chevron} data-open={open} viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
      <path d="M9 6l6 6-6 6" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function isClaimed(name: string): boolean {
  return !!name && !name.includes('#') && !/^0x[0-9a-f]+$/i.test(name)
}

export function ProfileCard({
  user,
  x,
  y,
  me,
  relationship = 'none',
  onFriendAction,
  onMention,
  onViewProfile,
  onReport,
  invitableCommunities,
  onRequestInvitable,
  onInvite,
  onClose
}: {
  user: ChatUser
  x: number
  y: number
  me?: { address?: string } | null
  /** Relationship of the local user to this profile — drives the friend CTA + Block/Unblock. */
  relationship?: Relationship
  /** Unified friend action (request/accept/reject/block/unblock). Absent → no friend controls. */
  onFriendAction?: (op: FriendAction, address: string) => void
  onMention?: (name: string) => void
  /** Open the full passport (labelled "View Passport"). */
  onViewProfile?: (user: ChatUser) => void
  onReport?: (user: ChatUser) => void
  /** Communities the local user can invite this address to (empty/undefined → hide the row). */
  invitableCommunities?: InvitableCommunity[]
  /** Fetch the invitable list for this address (called once when the card opens). */
  onRequestInvitable?: (address: string) => void
  onInvite?: (communityId: string, address: string) => void
  onClose: () => void
}): React.JSX.Element {
  const [copied, setCopied] = useState<'name' | 'address' | null>(null)
  const [justSent, setJustSent] = useState(false)
  const [inviteOpen, setInviteOpen] = useState(false)
  const [confirmReport, setConfirmReport] = useState(false)
  const [confirmBlock, setConfirmBlock] = useState(false)
  // After firing the friend request, show "REQUEST SENT" briefly, then close.
  useEffect(() => {
    if (!justSent) return
    const t = setTimeout(onClose, 1100)
    return () => clearTimeout(t)
  }, [justSent, onClose])
  // Reset the "copied" hint after a beat — in an effect so the timer is cleared on unmount.
  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(null), 1200)
    return () => clearTimeout(t)
  }, [copied])
  const cardRef = useRef<HTMLDivElement>(null)
  // Start at the click point; once the card is laid out, clamp it to the viewport using its REAL
  // size. Re-clamp when the height can change after mount — the relationship-driven CTA, the async
  // "Invite to Community" row (arrives via fetch), and expanding its submenu all grow the card, and
  // a stale clamp would push the lower rows off-screen.
  const [pos, setPos] = useState({ left: x, top: y })
  useLayoutEffect(() => {
    const el = cardRef.current
    if (!el) return
    const r = el.getBoundingClientRect()
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - r.height - 8))
    })
  }, [x, y, relationship, inviteOpen, invitableCommunities?.length])
  const isMe = !!me?.address && !!user.address && me.address.toLowerCase() === user.address.toLowerCase()
  const { base, tag } = splitName(user.name)
  const color = nameColor(user.address || user.name)
  // Ask which communities the local user can invite this address to (server filters by role);
  // an empty result hides the "Invite to Community" row (mirrors the old profile-menu).
  useEffect(() => {
    if (!isMe && user.address && onRequestInvitable) onRequestInvitable(user.address)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [user.address])

  const copy = (text: string, which: 'name' | 'address'): void => {
    navigator.clipboard?.writeText(text).then(
      () => setCopied(which),
      () => {}
    )
  }

  const hasInvite = !!onInvite && !!user.address && !!invitableCommunities && invitableCommunities.length > 0
  const hasDestructive = (!!onFriendAction && !!user.address) || !!onReport
  const hasMenu = !isMe && (!!onViewProfile || !!onMention || hasInvite || hasDestructive)

  return createPortal(
    <>
      {/* Hide the card (not unmount — keeps its state) while a destructive confirm is up, so the
          popup isn't stuck behind it. Proper stacking is backlog item 9 (consolidate modals + z-layer). */}
      <div className={styles.scrim} onClick={onClose} style={confirmReport || confirmBlock ? { display: 'none' } : undefined} />
      <div ref={cardRef} className={styles.card} style={confirmReport || confirmBlock ? { display: 'none' } : { left: pos.left, top: pos.top }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Profile">
        <div className={styles.header}>
          <Avatar src={user.picture} name={base} color={color} size={72} status="online" />
          <button type="button" className={styles.copyRow} title="Copy name" onClick={() => copy(user.name, 'name')}>
            <span className={styles.name} style={{ color }}>
              {base}
              {tag && <span className={styles.tag}>{tag}</span>}
            </span>
            {isClaimed(user.name) && <Verified />}
            <CopyIcon />
          </button>
          {user.address && (
            <button type="button" className={`${styles.copyRow} ${styles.addrRow}`} title="Copy address" onClick={() => copy(user.address, 'address')}>
              <span className={styles.addr}>{shortAddr(user.address)}</span>
              <CopyIcon />
            </button>
          )}
          {copied && <span className={styles.copied}>Copied {copied}</span>}
        </div>

        {!isMe && user.address && onFriendAction && relationship !== 'friend' && relationship !== 'blocked' && (
          relationship === 'incoming' ? (
            <div className={styles.ctaRow}>
              <button type="button" className={styles.cta} onClick={() => { onFriendAction('accept', user.address); onClose() }}>
                ACCEPT
              </button>
              <button type="button" className={styles.ctaGhost} onClick={() => { onFriendAction('reject', user.address); onClose() }}>
                REJECT
              </button>
            </div>
          ) : justSent || relationship === 'requested' ? (
            <div className={`${styles.cta} ${styles.ctaSent}`}>✓ REQUEST SENT</div>
          ) : (
            <button type="button" className={styles.cta} onClick={() => { onFriendAction('request', user.address); setJustSent(true) }}>
              <AddFriendIcon /> ADD FRIEND
            </button>
          )
        )}

        {hasMenu && (
          <div className={styles.menu}>
            {onViewProfile && (
              <button type="button" className={styles.row} onClick={() => { onViewProfile(user); onClose() }}>
                <ViewProfileIcon />
                <span>View Passport</span>
              </button>
            )}
            {onMention && (
              <button type="button" className={styles.row} onClick={() => { onMention(base); onClose() }}>
                <MentionIcon />
                <span>Mention</span>
              </button>
            )}
            {hasInvite && (
              <>
                <button type="button" className={styles.row} aria-expanded={inviteOpen} onClick={() => setInviteOpen((o) => !o)}>
                  <CommunityIcon />
                  <span>Invite to Community</span>
                  <Chevron open={inviteOpen} />
                </button>
                {inviteOpen && (
                  <div className={styles.submenu}>
                    {invitableCommunities!.map((c) => (
                      <button key={c.id} type="button" className={styles.subRow} onClick={() => { onInvite!(c.id, user.address); onClose() }}>
                        {c.name}
                      </button>
                    ))}
                  </div>
                )}
              </>
            )}
            {hasDestructive && <div className={styles.divider} />}
            {onFriendAction && user.address && (
              relationship === 'blocked' ? (
                <button type="button" className={styles.row} onClick={() => { onFriendAction('unblock', user.address); onClose() }}>
                  <BlockIcon />
                  <span>Unblock</span>
                </button>
              ) : (
                <button type="button" className={`${styles.row} ${styles.danger}`} onClick={() => setConfirmBlock(true)}>
                  <BlockIcon />
                  <span>Block</span>
                </button>
              )
            )}
            {onReport && (
              <button type="button" className={`${styles.row} ${styles.danger}`} onClick={() => setConfirmReport(true)}>
                <ReportIcon />
                <span>Report</span>
              </button>
            )}
          </div>
        )}
      </div>
      {confirmReport && (
        <ModalShell
          title={`Report ${base}?`}
          onClose={() => setConfirmReport(false)}
          width={420}
          actions={
            <>
              <Button variant="ghost" onClick={() => setConfirmReport(false)}>
                Cancel
              </Button>
              <Button variant="primary" onClick={() => { onReport?.(user); setConfirmReport(false); onClose() }}>
                Report
              </Button>
            </>
          }
          actionsEqual
        >
          Reports help moderators take action against users that break Decentraland&apos;s Community Guidelines.
        </ModalShell>
      )}
      {confirmBlock && (
        <ModalShell
          title={`Block ${base}?`}
          onClose={() => setConfirmBlock(false)}
          width={420}
          actions={
            <>
              <Button variant="ghost" onClick={() => setConfirmBlock(false)}>
                Cancel
              </Button>
              <Button variant="primary" onClick={() => { onFriendAction?.('block', user.address); setConfirmBlock(false); onClose() }}>
                Block
              </Button>
            </>
          }
          actionsEqual
        >
          Blocked users won&apos;t be able to message you, join your community events, or see when you&apos;re online.
        </ModalShell>
      )}
    </>,
    document.body
  )
}
