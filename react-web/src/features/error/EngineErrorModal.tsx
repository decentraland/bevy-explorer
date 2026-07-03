// Full-screen error popup shown when the engine panics/crashes, react-web itself throws, or a
// requested world doesn't exist. Sits above everything (login/loading). Fed by useEngineSession's
// fatalError or the ErrorBoundary fallback.

import { ModalShell, Button } from '../../design'
import styles from './EngineErrorModal.module.css'

export interface FatalError {
  message: string
  /**
   * 'launch' = boot panic (fatal), 'runtime' = engine crash (dismissable), 'react' = UI render
   * crash (fatal), 'realm' = the requested ?realm/world doesn't exist (dismissable → picker).
   */
  source: 'launch' | 'runtime' | 'react' | 'realm'
}

const COPY: Record<FatalError['source'], { title: string; subtitle: string }> = {
  launch: { title: 'Something went wrong', subtitle: "The 3D engine couldn't start. Reloading often fixes it." },
  runtime: { title: 'The world crashed', subtitle: 'The 3D engine stopped unexpectedly.' },
  react: { title: 'Something went wrong', subtitle: 'The app hit an unexpected error.' },
  // The realm message is human-readable and names the world — it IS the subtitle.
  realm: { title: 'World not found', subtitle: '' }
}

export function EngineErrorModal({
  error,
  onReload,
  onDismiss
}: {
  error: FatalError
  onReload: () => void
  /** Provided for dismissable errors (runtime crash, bad realm) — a boot/react crash is fatal. */
  onDismiss?: () => void
}): React.JSX.Element {
  const { title, subtitle } = COPY[error.source]
  // A bad realm isn't a crash: no technical detail, no Reload (it would re-hit the same URL) —
  // just OK back to the destination picker. Crashes show the panic text (bug reports) + Reload.
  const isRealm = error.source === 'realm'

  return (
    <ModalShell
      // Alert-style dialog: centered header with title-scale type (the shell's default header is
      // a compact panel heading), centered footer buttons.
      header={
        <div className={styles.head}>
          <h2 className={styles.title}>{title}</h2>
          <p className={styles.subtitle}>{isRealm ? error.message : subtitle}</p>
        </div>
      }
      role="alertdialog"
      ariaLabel={title}
      // A dismissable error can be closed via Escape or scrim-click; a fatal boot/react crash
      // has no onDismiss → onClose is undefined → it can't be escaped. No header X — footer buttons
      // drive it. (ModalShell ties Escape to dismissOnScrim, so gate both on onDismiss.)
      onClose={onDismiss}
      dismissOnScrim={!!onDismiss}
      closeButton={false}
      // Sit above login/loading (Modal's default backdrop is --z-modal).
      backdropClassName={styles.fatalLayer}
      actionsAlign="center"
      actions={
        isRealm ? (
          <Button variant="primary" className={styles.btn} onClick={onDismiss}>
            OK
          </Button>
        ) : (
          <>
            {onDismiss && (
              <Button variant="ghost" className={styles.btn} onClick={onDismiss}>
                Dismiss
              </Button>
            )}
            <Button variant="primary" className={styles.btn} onClick={onReload}>
              Reload
            </Button>
          </>
        )
      }
    >
      {!isRealm && <pre className={styles.detail}>{error.message}</pre>}
    </ModalShell>
  )
}
