// World password prompt — a password-protected world the player is travelling to. The engine
// holds the travel until this is answered (ipfs::world_access): the password goes back as a
// worldPasswordResolve, and a refused one comes back as a fresh prompt with the reason. Same
// violet card as the scene permission prompt.

import { useState } from 'react'
import { Button, TextInput, openPopup } from '../../design'
import type { WorldPasswordRequestMessage } from '../../engine/protocol'
import styles from './WorldPasswordDialog.module.css'

function LockIcon(): React.JSX.Element {
  return (
    <svg width="26" height="26" viewBox="0 0 24 24" aria-hidden="true" fill="none">
      <rect x="4" y="10" width="16" height="11" rx="2" fill="currentColor" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}

export function WorldPasswordDialog({
  request,
  onResolve
}: {
  request: WorldPasswordRequestMessage
  onResolve: (password: string | null) => void
}): React.JSX.Element {
  const [password, setPassword] = useState('')
  const submit = (): void => {
    if (password.length > 0) onResolve(password)
  }

  // Just the card — the popup layer draws the scrim, and dismissing it (Escape / scrim-click) gives
  // up via openWorldPasswordDialog's onClose. stopPropagation keeps a click on the card from dismissing.
  return (
    <form
      className={styles.modal}
      role="dialog"
      aria-modal="true"
      aria-label="World password"
      onClick={(e) => e.stopPropagation()}
      onSubmit={(e) => {
        e.preventDefault()
        submit()
      }}
    >
      <span className={styles.icon}>
        <LockIcon />
      </span>
      <div className={styles.prompt}>
        <span className={styles.world}>{request.world}</span> is password protected
      </div>
      <div className={styles.hint}>Enter the password to join this world</div>
      <TextInput
        className={styles.field}
        type="password"
        aria-label="Password"
        placeholder="Password"
        autoFocus
        autoComplete="off"
        value={password}
        invalid={request.error != null}
        onChange={setPassword}
      />
      {request.error ? (
        <div className={styles.error} role="alert">
          {request.error}
        </div>
      ) : null}
      <div className={styles.actions}>
        <Button variant="ghost" type="button" className={styles.cancel} onClick={() => onResolve(null)}>
          Cancel
        </Button>
        <Button variant="primary" type="submit" disabled={password.length === 0}>
          Enter
        </Button>
      </div>
    </form>
  )
}

/** Open a world password prompt as a popup; returns the close handle. Escape / scrim-click give up
 *  (the travel is refused). Resolves at most once, and not at all when closed through the handle:
 *  the owner closing it (a newer prompt replacing this one, or the engine withdrawing it) is not an
 *  answer. */
export function openWorldPasswordDialog(
  request: WorldPasswordRequestMessage,
  resolve: (password: string | null) => void
): () => void {
  let settled = false
  const done = (password: string | null): void => {
    if (settled) return
    settled = true
    resolve(password)
  }
  const close = openPopup(
    (close) => (
      <WorldPasswordDialog
        request={request}
        onResolve={(password) => {
          done(password)
          close()
        }}
      />
    ),
    { onClose: () => done(null) }
  )
  return () => {
    settled = true
    close()
  }
}
