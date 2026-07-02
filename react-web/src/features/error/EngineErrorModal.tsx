// Full-screen error popup shown when the engine panics/crashes or react-web itself throws. Sits above
// everything (login/loading). Fed by useEngineSession's fatalError or the ErrorBoundary fallback.

import { useEffect, useRef, useState } from 'react'
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
  realm: { title: 'World not found', subtitle: "That world doesn't exist or isn't reachable right now." }
}

export function EngineErrorModal({
  error,
  onReload,
  onDismiss
}: {
  error: FatalError
  onReload: () => void
  /** Provided only for dismissable (runtime) errors — a boot panic is fatal. */
  onDismiss?: () => void
}): React.JSX.Element {
  const [copied, setCopied] = useState(false)
  // Reset the "Copied" label after a beat; cleared on unmount so it can't fire on a gone component
  // (Reload navigates, or a runtime crash is dismissed, within the window).
  const copiedTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => () => { if (copiedTimer.current) clearTimeout(copiedTimer.current) }, [])
  const { title, subtitle } = COPY[error.source]

  const copy = (): void => {
    // clipboard.writeText rejects when the document isn't focused (e.g. right after an alt-tab back
    // to the crashed tab) — handle the rejection so it's not an unhandled promise error.
    navigator.clipboard?.writeText(error.message).then(
      () => {
        setCopied(true)
        if (copiedTimer.current) clearTimeout(copiedTimer.current)
        copiedTimer.current = setTimeout(() => setCopied(false), 1200)
      },
      () => {}
    )
  }

  return (
    <ModalShell
      title={title}
      subtitle={subtitle}
      role="alertdialog"
      ariaLabel={title}
      // A dismissable (runtime) crash can be closed via Escape or scrim-click; a fatal boot/react crash
      // has no onDismiss → onClose is undefined → it can't be escaped. No header X — footer buttons drive it.
      // (ModalShell ties Escape to dismissOnScrim, so gate both on onDismiss.)
      onClose={onDismiss}
      dismissOnScrim={!!onDismiss}
      closeButton={false}
      // Sit above login/loading (Modal's default backdrop is --z-modal).
      backdropClassName={styles.fatalLayer}
      actions={
        <>
          {onDismiss && (
            <Button variant="ghost" onClick={onDismiss}>
              Dismiss
            </Button>
          )}
          <Button variant="secondary" onClick={copy}>
            {copied ? 'Copied' : 'Copy details'}
          </Button>
          <Button variant="primary" onClick={onReload}>
            Reload
          </Button>
        </>
      }
    >
      <pre className={styles.detail}>{error.message}</pre>
    </ModalShell>
  )
}
