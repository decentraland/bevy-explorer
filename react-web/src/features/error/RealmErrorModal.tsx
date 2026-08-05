// "World not found" — the realm the user asked for doesn't exist or isn't reachable.
//
// Not a crash: nothing is broken, the app just can't honour the request, so this is an ordinary
// dialog on the popup layer (PopupHost owns the scrim, entrance, focus trap and Escape) rather than
// the full-screen CrashModal. Nothing is frozen behind it — see inputLock for why that distinction
// matters. OK returns the user to the destination picker; there's no Reload, it would re-hit the
// same URL.

import { openPopup, ModalShell, Button } from '../../design'
import styles from './RealmErrorModal.module.css'

/** Open the world-not-found dialog; returns the close handle. `onDismiss` runs exactly once, on ANY
 *  close path (OK, Escape, scrim click) — the caller clears the error state there, so a keyboard
 *  dismiss can't strand the session holding a stale error. */
export function openRealmError(opts: { message: string; onDismiss: () => void }): () => void {
  let settled = false
  const done = (): void => {
    if (settled) return
    settled = true
    opts.onDismiss()
  }
  return openPopup(
    (close) => (
      <ModalShell
        // Alert-style dialog: centered header with title-scale type, centered footer button.
        header={
          <div className={styles.head}>
            <h2 className={styles.title}>World not found</h2>
            {/* The realm message is human-readable and names the world — it IS the subtitle. */}
            <p className={styles.subtitle}>{opts.message}</p>
          </div>
        }
        role="alertdialog"
        ariaLabel="World not found"
        closeButton={false}
        actionsAlign="center"
        actions={
          <Button variant="primary" className={styles.btn} onClick={close}>
            OK
          </Button>
        }
      >
        {/* No body: the message lives in the header as the subtitle, and there's no panic text to
            show. The empty body block keeps the header/footer spacing the crash card has. */}
        {null}
      </ModalShell>
    ),
    { onClose: done }
  )
}
