// The engine's boot progress, along the bottom of the page while it boots, whatever is on the page:
// the real weighted progress from the engine loader (deploy/web/ui.js; the bundle's own bar is
// hidden via hideLoader=1), held short of done once the engine can take commands until the first
// avatar has loaded (the engine is launched the moment it can be, and the stage shows a stand-in
// until then), then full for a moment before it goes.

import { useEffect, useState } from 'react'
import type { LoginFlow } from '../session/useEngineSession'
import styles from './BootProgress.module.css'

// Engine boot steps surfaced from the engine loader.
const STEP_LABEL: Record<string, string> = {
  download: 'Downloading engine',
  compile: 'Compiling',
  init: 'Initializing',
  workers: 'Starting workers',
  gpu: 'Preparing graphics'
}
// Where the bar waits for the first avatar, and how long it shows full once it has loaded.
const AVATAR_HOLD_PCT = 95
const DONE_FLASH_MS = 600

export function BootProgress({ flow }: { flow: LoginFlow }): React.JSX.Element | null {
  const done = flow.engineReady && flow.avatarLoaded
  const [hidden, setHidden] = useState(false)
  useEffect(() => {
    if (!done || hidden) return
    const id = setTimeout(() => setHidden(true), DONE_FLASH_MS)
    return () => clearTimeout(id)
  }, [done, hidden])
  // a boot that failed (a sign-in error shows instead) has no avatar to wait for
  if (hidden || flow.error != null) return null
  const label = done ? 'Ready' : ((flow.loadStep != null ? STEP_LABEL[flow.loadStep] : undefined) ?? 'Starting…')
  const pct = done ? 100 : Math.min(AVATAR_HOLD_PCT, Math.round(flow.loadProgress))
  return (
    <div className={styles.bootBar} role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} aria-label={label}>
      <div className={styles.bootLabel}>
        {label} · {pct}%
      </div>
      <div className={styles.bootTrack}>
        <div className={styles.bootFill} style={{ width: `${pct}%` }} />
      </div>
    </div>
  )
}
