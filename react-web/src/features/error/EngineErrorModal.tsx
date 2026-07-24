// Full-screen error popup shown when the engine panics/crashes, react-web itself throws, or a
// requested world doesn't exist. Sits above everything (login/loading). Fed by useEngineSession's
// fatalError or the ErrorBoundary fallback.

import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { ModalShell, Button } from '../../design'
import { useFocusTrap } from '../../lib/useFocusTrap'
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
  const scrimRef = useRef<HTMLDivElement>(null)
  useFocusTrap(scrimRef, true) // self-contained: no PopupHost here (ErrorBoundary fallback / embedded)

  // A dismissable error (runtime crash, bad realm) closes on Escape / scrim-click; a fatal boot/react
  // crash has no onDismiss and can't be escaped.
  useEffect(() => {
    if (!onDismiss) return
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.stopPropagation()
      onDismiss()
    }
    document.addEventListener('keydown', onKey, true)
    return () => document.removeEventListener('keydown', onKey, true)
  }, [onDismiss])

  // Own scrim at --z-fatal (above the whole popup layer + login), portaled to <body>. ModalShell is
  // rendered scrimless — just the card; this component owns the backdrop, DPI scale, animation, focus
  // trap and Escape (what PopupHost does for popups, but here without depending on it).
  return createPortal(
    <div ref={scrimRef} className={styles.scrim} tabIndex={-1} onClick={onDismiss ?? undefined}>
      <div className={styles.pop}>
        <ModalShell
          // Alert-style dialog: centered header with title-scale type, centered footer buttons.
          header={
            <div className={styles.head}>
              <h2 className={styles.title}>{title}</h2>
              <p className={styles.subtitle}>{isRealm ? error.message : subtitle}</p>
            </div>
          }
          role="alertdialog"
          ariaLabel={title}
          closeButton={false}
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
      </div>
    </div>,
    document.body
  )
}
