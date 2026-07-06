// Create-a-Community form — faithfully ports dcl-react-ui's CommunityCreate: a NAME gate, then
// a profile picture, name*, a Public/Private membership dropdown, and a gradient discoverability
// bar. Submits via the bridge (signed multipart POST). Thumbnail upload is deferred — kernelFetch
// bodies are strings, so the chosen picture previews locally but isn't sent yet.

import { useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import styles from './CommunityCreateModal.module.css'

const MEMBERSHIP = [
  { value: 'public', label: 'Public', note: 'Anyone can become a member, view Community details, and join your Voice Streams' },
  { value: 'private', label: 'Private', note: 'Members must be approved by an owner or moderator before they can join' }
] as const

const NAMES_URL = 'https://decentraland.org/marketplace/names/claim'

export interface CreateCommunityInput {
  name: string
  description: string
  privacy: 'public' | 'private'
  discoverable: boolean
}

function LockArt(): React.JSX.Element {
  return (
    <svg viewBox="0 0 48 48" width="48" height="48" aria-hidden="true">
      <path d="M14 20v-4a10 10 0 0 1 20 0v4M10 20h28v18H10z" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}
function ImageIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="34" height="34" aria-hidden="true">
      <rect x="3" y="4" width="18" height="16" rx="2" fill="none" stroke="currentColor" strokeWidth="1.6" />
      <circle cx="8.5" cy="9.5" r="1.8" fill="currentColor" />
      <path d="M5 18l4.5-5 3.5 4 2.5-2.5L21 17" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}
function PencilIcon(): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
      <path d="M14.06 6.19l3.75 3.75M3 17.25V21h3.75L17.81 9.94a1.5 1.5 0 0 0 0-2.12l-1.63-1.63a1.5 1.5 0 0 0-2.12 0L3 17.25z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function CommunityCreateModal({
  canCreate,
  onCreate,
  onClose
}: {
  /** The user has a claimed NAME (community creation is gated behind one). */
  canCreate: boolean
  onCreate: (input: CreateCommunityInput) => void
  onClose: () => void
}): React.JSX.Element {
  const [name, setName] = useState('')
  const [privacy, setPrivacy] = useState<'public' | 'private'>('public')
  const [pfp, setPfp] = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const valid = name.trim().length > 0

  // Local-only preview; the picture isn't uploaded yet (see file header).
  const pickPfp = (e: React.ChangeEvent<HTMLInputElement>): void => {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => setPfp(typeof reader.result === 'string' ? reader.result : null)
    reader.readAsDataURL(file)
  }
  const submit = (): void => {
    if (!valid) return
    // The reference form has no discoverability control → default to listed (visibility 'all').
    onCreate({ name: name.trim(), description: '', privacy, discoverable: true })
    onClose()
  }

  return createPortal(
    <div className={styles.scrim} onClick={onClose}>
      {!canCreate ? (
        <div className={styles.gate} onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Get a NAME">
          <div className={styles.gateArt}><LockArt /></div>
          <h2 className={styles.gateTitle}>Get a NAME to Unlock Community Creation</h2>
          <p className={styles.gateBody}>
            NAMEs are unique Decentraland usernames that come with a World, and unlock community creation.
          </p>
          <div className={styles.gateBtns}>
            <button type="button" className={styles.primary} onClick={() => window.open(NAMES_URL, '_blank', 'noopener')}>Get a NAME</button>
            <button type="button" className={styles.ghost} onClick={onClose}>Maybe later</button>
          </div>
        </div>
      ) : (
        <div className={styles.modal} role="dialog" aria-modal="true" aria-label="Create a Community" onClick={(e) => e.stopPropagation()}>
          <h2 className={styles.title}>Create a Community</h2>

          <div className={styles.scroll}>
            <div className={styles.group}>
              <span className={styles.label}>PROFILE PICTURE</span>
              <span className={styles.hint}>PNG or JPG | 512x512 px | 500KB max</span>
              <div className={styles.pfp}>
                <span className={styles.pfpImg} style={pfp ? { backgroundImage: `url(${pfp})` } : undefined}>
                  {!pfp && <ImageIcon />}
                </span>
                <button type="button" className={styles.pfpEdit} aria-label="Edit profile picture" onClick={() => fileRef.current?.click()}>
                  <PencilIcon />
                </button>
                <input ref={fileRef} type="file" accept="image/png,image/jpeg" hidden onChange={pickPfp} />
              </div>
            </div>

            <div className={styles.group}>
              <label className={styles.label} htmlFor="cc-name">COMMUNITY NAME <span className={styles.req}>*</span></label>
              <input
                id="cc-name"
                className={styles.input}
                maxLength={30}
                placeholder="Write here"
                value={name}
                onChange={(e) => setName(e.target.value)}
                // eslint-disable-next-line jsx-a11y/no-autofocus
                autoFocus
              />
            </div>

            <div className={styles.group}>
              <label className={styles.label} htmlFor="cc-membership">MEMBERSHIP</label>
              <div className={styles.selectWrap}>
                <select id="cc-membership" className={styles.select} value={privacy} onChange={(e) => setPrivacy(e.target.value as 'public' | 'private')}>
                  {MEMBERSHIP.map((o) => (
                    <option key={o.value} value={o.value}>{o.label}  {o.note}</option>
                  ))}
                </select>
                <svg className={styles.chevron} viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
                  <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </div>
            </div>
          </div>

          <div className={styles.actions}>
            <button type="button" className={`${styles.ghost} ${styles.cancel}`} onClick={onClose}>CANCEL</button>
            <button type="button" className={`${styles.primary} ${styles.create}`} disabled={!valid} onClick={submit}>CREATE</button>
          </div>

          <p className={styles.policy}>Please ensure Community content follows Decentraland's Content Policy.</p>
        </div>
      )}
    </div>,
    document.body
  )
}
