// Profile menu — the popover the SDK7 chat opened when you clicked a sender's
// name/avatar or an @mention (Figma "Profile Menu", node 8337-2971). Header with
// avatar / name+copy / address+copy and an ADD FRIEND CTA, then the action list.
// We only render the actions our bridge actually supports: Mention, Block. (View
// Profile / Chat / Call / Hush / Gift / Report have no engine API yet.)

import { useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Avatar } from '../../design'
import { nameColor, shortAddr, splitName } from '../../lib/identity'
import styles from './ProfileCard.module.css'

export interface ChatUser {
  address: string
  name: string
  picture?: string
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

export function ProfileCard({
  user,
  x,
  y,
  me,
  onAddFriend,
  onMention,
  onBlock,
  onClose
}: {
  user: ChatUser
  x: number
  y: number
  me?: { address?: string } | null
  onAddFriend?: (address: string) => void
  onMention?: (name: string) => void
  onBlock?: (address: string) => void
  onClose: () => void
}): React.JSX.Element {
  const [copied, setCopied] = useState<'name' | 'address' | null>(null)
  const cardRef = useRef<HTMLDivElement>(null)
  // Start at the click point; once the card is laid out, clamp it to the viewport
  // using its REAL size (the menu height varies with which actions are shown).
  const [pos, setPos] = useState({ left: x, top: y })
  useLayoutEffect(() => {
    const el = cardRef.current
    if (!el) return
    const r = el.getBoundingClientRect()
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - r.height - 8))
    })
  }, [x, y])
  const isMe = !!me?.address && !!user.address && me.address.toLowerCase() === user.address.toLowerCase()
  const { base, tag } = splitName(user.name)
  const color = nameColor(user.address || user.name)

  const copy = (text: string, which: 'name' | 'address'): void => {
    navigator.clipboard?.writeText(text).then(
      () => {
        setCopied(which)
        setTimeout(() => setCopied(null), 1200)
      },
      () => {}
    )
  }

  return createPortal(
    <>
      <div className={styles.scrim} onClick={onClose} />
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
        </div>

        {!isMe && user.address && onAddFriend && (
          <button type="button" className={styles.cta} onClick={() => { onAddFriend(user.address); onClose() }}>
            <AddFriendIcon /> ADD FRIEND
          </button>
        )}

        {!isMe && (onMention || onBlock) && (
          <div className={styles.menu}>
            {onMention && (
              <button type="button" className={styles.row} onClick={() => { onMention(base); onClose() }}>
                <MentionIcon />
                <span>Mention</span>
              </button>
            )}
            {onBlock && user.address && (
              <>
                <div className={styles.divider} />
                <button type="button" className={`${styles.row} ${styles.danger}`} onClick={() => { onBlock(user.address); onClose() }}>
                  <BlockIcon />
                  <span>Block</span>
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </>,
    document.body
  )
}
