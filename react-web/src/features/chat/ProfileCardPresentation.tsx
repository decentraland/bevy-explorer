// Profile card (the old SDK7 "profile menu") — the popover shown when you click a
// sender's name/avatar, an @mention, or a nearby avatar in the world. Header with
// avatar / name+copy / address+copy, a relationship-driven friend CTA (Add / Accept +
// Reject / Requested), then the action list — mirroring bevy-ui-scene's profile-menu:
// View Passport · Mention · Block/Unblock.

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Avatar } from '../../design'
import { shortAddr, splitName, userColor } from '../../lib/identity'
import type { Relationship } from '../../lib/relationship'
import styles from './ProfileCard.module.css'

export type { Relationship } from '../../lib/relationship'

export interface ChatUser {
  address: string
  name: string
  picture?: string
  claimed?: boolean
  nameColor?: { r: number; g: number; b: number }
}

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

function isClaimed(name: string): boolean {
  return !!name && !name.includes('#') && !/^0x[0-9a-f]+$/i.test(name)
}

export type MenuContext = 'default' | 'friend' | 'request' | 'blocked'

function ReportIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <path d="M5 21V4h11l-1.5 4L16 12H5" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
    </svg>
  )
}

export function ProfileCardPresentation({
  user,
  x,
  y,
  above = false,
  me,
  relationship = 'none',
  context = 'default',
  onAddFriend,
  onUnfriend,
  onCancelRequest,
  onAcceptRequest,
  onBlock,
  onReport,
  onMention,
  onViewProfile,
  onClose
}: {
  user: ChatUser
  x: number
  y: number
  /** Grow upward from (x, y) — anchored to a row's menu button — instead of down from a click. */
  above?: boolean
  me?: { address?: string } | null
  /** Relationship of the local user to this profile — drives the friendship button. */
  relationship?: Relationship
  /** Which list opened it: the item set follows the reference menu for that list. */
  context?: MenuContext
  onAddFriend?: (user: ChatUser) => void
  onUnfriend?: (user: ChatUser) => void
  onCancelRequest?: (user: ChatUser) => void
  onAcceptRequest?: (user: ChatUser) => void
  onBlock?: (user: ChatUser) => void
  onReport?: (user: ChatUser) => void
  onMention?: (name: string) => void
  onViewProfile?: (user: ChatUser) => void
  onClose: () => void
}): React.JSX.Element {
  const [copied, setCopied] = useState<'name' | 'address' | null>(null)
  // Reset the "copied" hint after a beat — in an effect so the timer is cleared on unmount.
  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(null), 1200)
    return () => clearTimeout(t)
  }, [copied])
  const cardRef = useRef<HTMLDivElement>(null)
  // Start at the anchor; once laid out, clamp to the viewport using the card's REAL size.
  const [pos, setPos] = useState({ left: x, top: y })
  useLayoutEffect(() => {
    const el = cardRef.current
    if (!el) return
    const r = el.getBoundingClientRect()
    const top = above ? y - r.height : y
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
      top: Math.max(8, Math.min(top, window.innerHeight - r.height - 8))
    })
  }, [x, y, above, relationship])
  const isMe = !!me?.address && !!user.address && me.address.toLowerCase() === user.address.toLowerCase()
  const { base, tag } = splitName(user.name)
  const color = userColor(user.address, user.name, user.claimed, user.nameColor)

  const copy = (text: string, which: 'name' | 'address'): void => {
    navigator.clipboard?.writeText(text).then(
      () => setCopied(which),
      () => {}
    )
  }
  const run = (fn?: (u: ChatUser) => void) => (): void => {
    onClose()
    fn?.(user)
  }

  const other = !isMe && !!user.address
  const canBlock = other && relationship !== 'blocked' && !!onBlock
  const canReport = other && !!onReport
  const showMention = context !== 'request' && context !== 'blocked' && !!onMention

  let cta: React.ReactNode = null
  if (other && relationship !== 'blocked') {
    if (relationship === 'friend' && onUnfriend) {
      cta = (
        <button type="button" className={`${styles.cta} ${styles.ctaFriend}`} onClick={run(onUnfriend)}>
          <span className={styles.ctaIdle}>Friend</span>
          <span className={styles.ctaHover}>Remove Friend</span>
        </button>
      )
    } else if (relationship === 'requested' && onCancelRequest) {
      cta = (
        <button type="button" className={`${styles.cta} ${styles.ctaSent}`} onClick={run(onCancelRequest)}>
          <span className={styles.ctaIdle}>Request Sent</span>
          <span className={styles.ctaHover}>Cancel Request</span>
        </button>
      )
    } else if (relationship === 'incoming' && onAcceptRequest) {
      cta = (
        <button type="button" className={`${styles.cta} ${styles.ctaPrimary}`} onClick={run(onAcceptRequest)}>
          Accept Friend
        </button>
      )
    } else if (relationship === 'none' && onAddFriend) {
      cta = (
        <button type="button" className={`${styles.cta} ${styles.ctaPrimary}`} onClick={run(onAddFriend)}>
          <AddFriendIcon /> Add Friend
        </button>
      )
    }
  }

  const viewProfile = onViewProfile && (
    <button type="button" className={styles.row} onClick={run(onViewProfile)}>
      <ViewProfileIcon />
      <span>View Profile</span>
    </button>
  )
  const block = canBlock && (
    <button type="button" className={`${styles.row} ${styles.danger}`} onClick={run(onBlock)}>
      <BlockIcon />
      <span>Block</span>
    </button>
  )
  const report = canReport && (
    <button type="button" className={`${styles.row} ${styles.danger}`} onClick={run(onReport)}>
      <ReportIcon />
      <span>Report</span>
    </button>
  )
  const hasDestructive = !!block || !!report
  const hasMenu = !isMe && (!!viewProfile || showMention || hasDestructive)

  // The backdrop (and click-outside-to-close) is owned by the popup layer (openPopup default
  // options); this just renders the positioned card. The stopPropagation keeps a click on the card
  // from reaching the backdrop.
  return (
    <div ref={cardRef} className={styles.card} style={{ left: pos.left, top: pos.top }} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Profile">
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
        {context !== 'blocked' && cta}
      </div>

      {hasMenu && (
        <div className={styles.menu}>
          <div className={styles.divider} />
          {context === 'request' ? (
            <>
              {viewProfile}
              {hasDestructive && <div className={styles.divider} />}
              {block}
              {report}
            </>
          ) : context === 'blocked' ? (
            viewProfile
          ) : (
            <>
              {showMention && (
                <button type="button" className={styles.row} onClick={() => { onMention?.(base); onClose() }}>
                  <MentionIcon />
                  <span>Mention</span>
                </button>
              )}
              {viewProfile}
              {hasDestructive && <div className={styles.divider} />}
              {report}
              {block}
            </>
          )}
        </div>
      )}
    </div>
  )
}
