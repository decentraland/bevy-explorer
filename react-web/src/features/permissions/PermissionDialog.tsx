// Scene permission prompt — restores the "the scene wants permission to …" dialog (e.g. a scene
// asking to move you to a new realm) that the native Bevy HUD shows but the react-web HUD lost.
// The engine relays the request over the bridge (domains/permissions.ts); the user's choice goes
// back as a permissionResolve. Violet design-system treatment, matching the Create-a-Community
// modal; wording + Scene/Realm/Global scopes mirror unity-explorer / the native dialog, with
// "This visit" (the page remembers the answer for the scene) in place of their "Once".
// The same card, with the same scopes, reviews a permission the scene already used (the rail's
// permissions list): there is no request to answer, so the choice only rewrites the rule
// (permissionRule) for next time.

import { useState } from 'react'
import { Button, openPopup } from '../../design'
import type { PermissionLevelChoice, PermissionRequestMessage, PermissionRuleLevel } from '../../engine/protocol'
import type { PermissionSubject } from '../session/useEngineSession'
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
  OpenUrl: 'open a url in your browser',
  OpenExplorerUi: 'open an explorer menu panel (map, backpack, settings, ...)'
}

/** The "wants permission to …" clause for a permission type. */
export function describePermission(ty: string): string {
  return PASSIVE[ty] ?? 'perform a restricted action'
}

const LEVELS: { value: PermissionRuleLevel; label: string }[] = [
  { value: 'visit', label: 'This visit' },
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

type Mode =
  | { kind: 'request'; request: PermissionRequestMessage; onResolve: (allow: boolean, level: PermissionLevelChoice) => void }
  | { kind: 'review'; subject: PermissionSubject; initial: PermissionRuleLevel; onRule: (allow: boolean, level: PermissionRuleLevel) => void }

function Card({
  label,
  prompt,
  additional,
  initial,
  onChoose
}: {
  label: string
  prompt: React.ReactNode
  additional?: string
  initial: PermissionRuleLevel
  onChoose: (allow: boolean, level: PermissionRuleLevel) => void
}): React.JSX.Element {
  const [level, setLevel] = useState<PermissionRuleLevel>(initial)

  // Just the card — the popup layer draws the scrim, and dismissing it (Escape / scrim-click) is the
  // opener's onClose. stopPropagation keeps a click on the card from dismissing.
  return (
    <div className={styles.modal} role="alertdialog" aria-modal="true" aria-label={label} onClick={(e) => e.stopPropagation()}>
      <span className={styles.icon}>
        <LockIcon />
      </span>
      <div className={styles.prompt}>{prompt}</div>
      {additional ? <div className={styles.additional}>{additional}</div> : null}

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
        <Button variant="ghost" className={styles.deny} onClick={() => onChoose(false, level)}>
          Deny
        </Button>
        <Button variant="primary" onClick={() => onChoose(true, level)}>
          Allow
        </Button>
      </div>
    </div>
  )
}

export function PermissionDialog(mode: Mode): React.JSX.Element {
  if (mode.kind === 'request') {
    const { request, onResolve } = mode
    return (
      <Card
        label="Scene permission request"
        prompt={
          <>
            The scene <span className={styles.scene}>{request.sceneName || 'A scene'}</span> wants permission to {describePermission(request.ty)}
          </>
        }
        additional={request.additional}
        initial="visit"
        onChoose={onResolve}
      />
    )
  }
  const { subject, initial, onRule } = mode
  return <Card label="Scene permission review" prompt={describeSubject(subject)} initial={initial} onChoose={onRule} />
}

/** A visit answer or a scene rule: "The scene X may / may not / asks before it can …" (a visit
 *  answer, "… this visit"). A realm or global rule: "Scenes …". */
function describeSubject(subject: PermissionSubject): React.ReactNode {
  const passive = describePermission(subject.ty)
  if (subject.scope === 'visit' || subject.scope === 'scene') {
    const verb = subject.value === 'Allow' ? 'may' : subject.value === 'Deny' ? 'may not' : 'asks before it can'
    return (
      <>
        The scene <span className={styles.scene}>{subject.sceneName || 'A scene'}</span> {verb} {passive}
        {subject.scope === 'visit' ? ' this visit' : ''}
      </>
    )
  }
  const verb = subject.value === 'Allow' ? 'may' : subject.value === 'Deny' ? 'may not' : 'ask before they'
  return (
    <>
      Scenes{subject.scope === 'realm' ? ' in this realm' : ''} {verb} {passive}
    </>
  )
}

/** Open a scene-permission prompt as a popup; returns the close handle. Escape / scrim-click deny once
 *  (the prompt never silently grants); Allow/Deny carry the chosen scope. Resolves exactly once. */
export function openPermissionDialog(
  request: PermissionRequestMessage,
  resolve: (id: number, allow: boolean, level: PermissionLevelChoice) => void
): () => void {
  let settled = false
  const done = (allow: boolean, level: PermissionLevelChoice): void => {
    if (settled) return
    settled = true
    resolve(request.id, allow, level)
  }
  return openPopup(
    (close) => (
      <PermissionDialog
        kind="request"
        request={request}
        onResolve={(allow, level) => {
          done(allow, level)
          close()
        }}
      />
    ),
    { onClose: () => done(false, 'once') }
  )
}

/** Review a visit answer or a stored rule: the same card and scopes, applied to next time, opening
 *  on `initial` (a stored rule's own level). Dismissing it changes nothing. */
export function openPermissionReview(
  subject: PermissionSubject,
  setRule: (subject: PermissionSubject, allow: boolean, level: PermissionRuleLevel) => void,
  initial: PermissionRuleLevel = 'visit'
): () => void {
  return openPopup((close) => (
    <PermissionDialog
      kind="review"
      subject={subject}
      initial={initial}
      onRule={(allow, level) => {
        setRule(subject, allow, level)
        close()
      }}
    />
  ))
}
