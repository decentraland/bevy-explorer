// Full-screen error popup shown when the engine panics/crashes or react-web itself throws. Sits above
// everything (login/loading). Fed by useEngineSession's fatalError or the ErrorBoundary fallback.

import { useState } from 'react'
import { Button } from '../../design'
import styles from './EngineErrorModal.module.css'

export interface FatalError {
  message: string
  /** 'launch' = boot panic (fatal), 'runtime' = engine crash (dismissable), 'react' = UI render crash. */
  source: 'launch' | 'runtime' | 'react'
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
  const title = error.source === 'runtime' ? 'The world crashed' : 'Something went wrong'
  const subtitle =
    error.source === 'launch'
      ? "The 3D engine couldn't start. Reloading often fixes it."
      : error.source === 'runtime'
        ? 'The 3D engine stopped unexpectedly.'
        : 'The app hit an unexpected error.'

  const copy = (): void => {
    void navigator.clipboard?.writeText(error.message).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1200)
    })
  }

  return (
    <div className={styles.root} role="alertdialog" aria-modal="true" aria-label={title}>
      <div className={styles.card}>
        <h1 className={styles.title}>{title}</h1>
        <p className={styles.subtitle}>{subtitle}</p>
        <pre className={styles.detail}>{error.message}</pre>
        <div className={styles.actions}>
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
        </div>
      </div>
    </div>
  )
}
