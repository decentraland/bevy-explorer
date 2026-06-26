// Perf overlay — live page + engine FPS. Toggle with ?fps=1 or Ctrl+Shift+F.
// Page fps = React/main-thread frame rate (drops if the HUD starves the frame budget);
// engine fps = the bevy render loop (shown only when the engine is present).

import { useFps } from '../../lib/useFps'
import styles from './FpsMeter.module.css'

export function FpsMeter(): React.JSX.Element {
  const { page, engine, ms } = useFps(true)
  const tone = page >= 55 ? styles.good : page >= 30 ? styles.warn : styles.bad
  return (
    <div className={styles.meter} aria-hidden="true">
      <span className={`${styles.fps} ${tone}`}>
        {page}
        <span className={styles.unit}>fps</span>
      </span>
      <span className={styles.dim}>{ms}ms</span>
      {engine != null && (
        <span className={styles.dim}>
          engine <b className={styles.engine}>{engine}</b>
        </span>
      )}
    </div>
  )
}
