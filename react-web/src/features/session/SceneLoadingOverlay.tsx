// React scene-asset loading screen — replaces the engine's in-world loading UI.
// Driven by the bridge scene's getSceneLoadingUIStream relay.

import { useRef } from 'react'
import type { SceneLoadingState } from '../../engine/protocol'
import styles from './SceneLoadingOverlay.module.css'

export function SceneLoadingOverlay({
  scene
}: {
  scene: SceneLoadingState | null
}): React.JSX.Element {
  // Track the peak pending-asset count to render a sensible progress bar.
  const peak = useRef(0)
  const pending = scene?.pendingAssets ?? null
  if (pending != null && pending > peak.current) peak.current = pending

  const connecting = scene != null && !scene.realmConnected
  const known = pending != null && peak.current > 0
  const progress = known ? 1 - pending / peak.current : 0

  const title = connecting
    ? 'Reconnecting…'
    : scene?.title
      ? scene.title
      : 'Entering Decentraland'

  const subtitle = connecting
    ? 'Reconnecting to the realm'
    : pending != null && pending > 0
      ? `Loading scene assets · ${pending} remaining`
      : 'Preparing your surroundings'

  return (
    <div className={styles.root}>
      <div className={styles.logo} />
      <h2 className={styles.title}>{title}</h2>
      <p className={styles.subtitle}>{subtitle}</p>
      <div className={styles.track}>
        <div
          className={`${styles.fill}${known ? '' : ' ' + styles.indeterminate}`}
          style={known ? { width: `${Math.round(progress * 100)}%` } : undefined}
        />
      </div>
    </div>
  )
}
