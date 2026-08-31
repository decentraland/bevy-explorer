// The profile chip in the menu top bar + its dropdown (avatar, name, wallet, View
// Profile, Sign Out, Exit). Matches the Explorer 2.0 menu profile popover.

import { useEffect, useRef, useState } from 'react'
import { Avatar } from '../../design'
import { nameColor, shortAddr } from '../../lib/identity'
import styles from './ProfileChip.module.css'

function Verified(): React.JSX.Element {
  return (
    <svg className={styles.verified} viewBox="0 0 16 16" aria-label="verified">
      <defs>
        <linearGradient id="pcv" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ff2d55" />
          <stop offset="1" stopColor="#c640cd" />
        </linearGradient>
      </defs>
      <path
        d="M8 1l1.7 1.2 2.1-.2 1 1.8 1.9.9-.5 2 .9 1.9-1.6 1.4.1 2.1-2 .6-1.1 1.8-2-.7-2 .7-1.1-1.8-2-.6.1-2.1L1.6 8.6l.9-1.9-.5-2 1.9-.9 1-1.8 2.1.2z"
        fill="url(#pcv)"
      />
      <path d="M5.5 8l1.7 1.7L10.8 6" stroke="#fff" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function PowerIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <path d="M12 3v8" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      <path d="M6.6 6.8a7 7 0 1 0 10.8 0" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}

function ExitIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <path d="M14 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h8" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      <path d="M10 12h11m0 0-3.5-3.5M21 12l-3.5 3.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function CopyIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  )
}

export function ProfileChip({
  name,
  picture,
  address,
  claimed,
  onViewProfile,
  onSignOut,
  onExit
}: {
  name: string
  picture?: string
  address?: string
  claimed?: boolean
  onViewProfile: () => void
  onSignOut: () => void
  onExit: () => void
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [copied, setCopied] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent): void => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    window.addEventListener('mousedown', onDown)
    return () => window.removeEventListener('mousedown', onDown)
  }, [open])

  const copy = (): void => {
    if (!address) return
    navigator.clipboard?.writeText(address).then(
      () => {
        setCopied(true)
        setTimeout(() => setCopied(false), 1200)
      },
      () => {}
    )
  }

  const color = nameColor(address ?? name)
  return (
    <div className={styles.root} ref={ref}>
      <button type="button" className={styles.chip} onClick={() => setOpen((o) => !o)}>
        <Avatar src={picture} name={name} color={color} size={32} />
        <span className={styles.chipName}>{name}</span>
      </button>

      {open && (
        <div className={styles.menu}>
          <div className={styles.head}>
            <Avatar src={picture} name={name} color={color} size={72} />
            <div className={styles.name} style={{ color }}>
              {name}
              {claimed && <Verified />}
            </div>
            {address && (
              <>
                <div className={styles.walletLabel}>WALLET ADDRESS</div>
                <button type="button" className={styles.wallet} onClick={copy} title="Copy address">
                  {shortAddr(address)}
                  <CopyIcon />
                  {copied && <span className={styles.copied}>Copied</span>}
                </button>
              </>
            )}
            <button
              type="button"
              className={styles.viewProfile}
              onClick={() => {
                setOpen(false)
                onViewProfile()
              }}
            >
              VIEW PROFILE
            </button>
          </div>

          <div className={styles.actions}>
            <button type="button" className={styles.action} onClick={onSignOut}>
              <PowerIcon />
              SIGN OUT
            </button>
            <button type="button" className={`${styles.action} ${styles.exit}`} onClick={onExit}>
              <ExitIcon />
              EXIT
            </button>
          </div>

          <div className={styles.footer}>
            <a className={styles.footLink} href="https://decentraland.org/terms/" target="_blank" rel="noreferrer">
              Terms of Service
            </a>
            <a className={styles.footLink} href="https://decentraland.org/privacy/" target="_blank" rel="noreferrer">
              Privacy Policy
            </a>
          </div>
        </div>
      )}
    </div>
  )
}
