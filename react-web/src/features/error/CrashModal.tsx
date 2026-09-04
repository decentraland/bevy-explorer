// Full-screen crash surface: the engine panics, the engine stops responding, or react-web itself
// throws. Sits above everything (the popup layer, login, loading) and is deliberately NOT a popup —
// it renders in the ErrorBoundary fallback and in embedded mode, where PopupHost isn't mounted, so it
// owns its scrim, DPI scale, animation and focus trap. Fed by useEngineSession's fatalError or the
// ErrorBoundary. A world-not-found is not a crash and does not come here — see openRealmError.

import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { ModalShell, Button } from '../../design'
import { useFocusTrap } from '../../lib/useFocusTrap'
import { lockInput } from '../../lib/inputLock'
import type { CrashSource } from './fatalError'
import styles from './CrashModal.module.css'

const COPY: Record<CrashSource, { title: string; subtitle: string }> = {
  launch: { title: 'Something went wrong', subtitle: "The 3D engine couldn't start. Reloading often fixes it." },
  runtime: { title: 'The world crashed', subtitle: 'The 3D engine stopped unexpectedly.' },
  react: { title: 'Something went wrong', subtitle: 'The app hit an unexpected error.' }
}

export function CrashModal({
  error,
  onReload,
  onDismiss
}: {
  error: { message: string; source: CrashSource }
  onReload: () => void
  /** Provided for a dismissable crash (the runtime watchdog can false-positive) — a boot/react crash
   *  is fatal and has no way out but Reload. */
  onDismiss?: () => void
}): React.JSX.Element {
  const { title, subtitle } = COPY[error.source]
  const scrimRef = useRef<HTMLDivElement>(null)

  // Freeze HUD input while this is up: every global hotkey, the popup layer's Escape and its focus trap
  // stand down, so whatever was open when the crash landed stays open, untouched and screenshottable —
  // see inputLock. Declared before the trap below: hook effects/cleanups run in declaration order, so
  // on unmount the lock releases first — otherwise our own cleanup would see itself still "locked" and
  // skip restoring focus to the opener.
  useEffect(lockInput, [])

  useFocusTrap(scrimRef, true) // self-contained: no PopupHost here (ErrorBoundary fallback / embedded)

  // No Escape-to-dismiss here, unlike every popup: Escape is reflex in-world (release pointer lock,
  // close a menu), and a stray press would wipe the panic text before it's read. A dismissable crash
  // closes on Dismiss or a scrim click; a fatal boot/react crash has no onDismiss at all.

  // Own scrim at --z-fatal (above the whole popup layer + login), portaled to <body>. ModalShell is
  // rendered scrimless — just the card; this component owns the backdrop, DPI scale, animation and
  // focus trap (what PopupHost does for popups, but here without depending on it).
  return createPortal(
    <div ref={scrimRef} className={styles.scrim} tabIndex={-1} onClick={onDismiss ?? undefined}>
      <div className={styles.pop}>
        <ModalShell
          // Alert-style dialog: centered header with title-scale type, centered footer buttons.
          header={
            <div className={styles.head}>
              <h2 className={styles.title}>{title}</h2>
              <p className={styles.subtitle}>{subtitle}</p>
            </div>
          }
          role="alertdialog"
          ariaLabel={title}
          closeButton={false}
          actionsAlign="center"
          actions={
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
          }
        >
          {/* The panic text itself — what a bug report needs. */}
          <pre className={styles.detail}>{error.message}</pre>
        </ModalShell>
      </div>
    </div>,
    document.body
  )
}
