// The engine's boot progress, along the bottom of the page while it boots, whatever is on the page:
// the real weighted progress from the engine loader (deploy/web/ui.js; the bundle's own bar is
// hidden via hideLoader=1). Gone once the engine can take commands.

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

export function BootProgress({ flow }: { flow: LoginFlow }): React.JSX.Element | null {
  if (flow.engineReady) return null
  const label = (flow.loadStep != null ? STEP_LABEL[flow.loadStep] : undefined) ?? 'Starting…'
  const pct = Math.round(flow.loadProgress)
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
