// Scene permission prompt — restores the "the scene wants permission to …" dialog (e.g. a scene
// asking to move you to a new realm) that the native Bevy HUD shows but the react-web HUD lost.
// The engine relays the request over the bridge (domains/permissions.ts); the user's choice goes
// back as a permissionResolve. Violet design-system treatment, matching the Create-a-Community
// modal; wording + Once/Scene/Realm/Global scopes mirror unity-explorer / the native dialog.

import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { Button } from '../../design'
import type { PermissionLevelChoice, PermissionRequestMessage } from '../../engine/protocol'
import styles from './PermissionDialog.module.css'

// PermissionType (serde enum name) → the "wants permission to {…}" clause. Mirrors the engine's
// PermissionStrings::passive in crates/common/src/structs.rs. Unknown types get a generic fallback.
const PASSIVE: Record<string, string> = {
  MovePlayer: 'move your avatar within the scene bounds',
  ForceCamera: 'temporarily change the camera view',
  PlayEmote: 'make your avatar perform an emote',
  SetLocomotion: "temporarily modify your avatar's locomotion settings",
  HideAvatarsNametags: 'temporarily hide player avatars and/or nametags, and/or disables passports',
  DisableVoice: 'temporarily disable voice chat',
  Teleport: 'teleport you to a new location',
  ChangeRealm: 'move you to a new realm',
  SpawnPortable: 'spawn a portable experience',
  KillPortables: 'manage your active portable experiences',
  Web3: 'initiate a web3 transaction with your wallet',
  CopyToClipboard: 'copy text into the clipboard',
  Fetch: 'fetch data from a remote server',
  Websocket: 'open a web socket to communicate with a remote server',
  OpenUrl: 'open a url in your browser'
}

const LEVELS: { value: PermissionLevelChoice; label: string }[] = [
  { value: 'once', label: 'Once' },
  { value: 'scene', label: 'Always for Scene' },
  { value: 'realm', label: 'Always for Realm' },
  { value: 'global', label: 'Always for Global' }
]

function LockIcon(): React.JSX.Element {
  return (
    <svg width="26" height="26" viewBox="0 0 24 24" aria-hidden="true" fill="none">
      <rect x="4" y="10" width="16" height="11" rx="2" fill="currentColor" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}

export function PermissionDialog({
  request,
  onResolve
}: {
  request: PermissionRequestMessage
  onResolve: (allow: boolean, level: PermissionLevelChoice) => void
}): React.JSX.Element {
  const [level, setLevel] = useState<PermissionLevelChoice>('once')
  const passive = PASSIVE[request.ty] ?? 'perform a restricted action'

  // Escape dismisses as a one-time deny — the prompt never silently grants.
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault()
        onResolve(false, 'once')
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onResolve])

  return createPortal(
    // Clicking the scrim is also a one-time deny.
    <div className={styles.scrim} onClick={() => onResolve(false, 'once')}>
      <div
        className={styles.modal}
        role="alertdialog"
        aria-modal="true"
        aria-label="Scene permission request"
        onClick={(e) => e.stopPropagation()}
      >
        <span className={styles.icon}>
          <LockIcon />
        </span>
        <div className={styles.prompt}>
          The scene <span className={styles.scene}>{request.sceneName || 'A scene'}</span> wants
          permission to {passive}
        </div>
        {request.additional ? <div className={styles.additional}>{request.additional}</div> : null}

        <div className={styles.options} role="radiogroup" aria-label="Apply this decision">
          {LEVELS.map((opt) => (
            <label key={opt.value} className={styles.option}>
              <input
                className={styles.input}
                type="radio"
                name="permission-level"
                checked={level === opt.value}
                onChange={() => setLevel(opt.value)}
              />
              <span className={`${styles.radio} ${level === opt.value ? styles.radioOn : ''}`.trim()}>
                {level === opt.value ? <span className={styles.radioDot} /> : null}
              </span>
              {opt.label}
            </label>
          ))}
        </div>

        <div className={styles.actions}>
          <Button variant="primary" onClick={() => onResolve(true, level)}>
            Allow
          </Button>
          <Button variant="ghost" className={styles.deny} onClick={() => onResolve(false, level)}>
            Deny
          </Button>
        </div>
      </div>
    </div>,
    document.body
  )
}
