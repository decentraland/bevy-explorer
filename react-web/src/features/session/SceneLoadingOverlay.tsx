// React scene-asset loading screen — replaces the engine's in-world loading UI.
// Driven by the bridge scene's getSceneLoadingUIStream relay. Layout follows unity-explorer's
// SceneLoadingScreenView: a top bar with LOADING N% over a progress line, and a tips carousel.

import { useEffect, useRef, useState } from 'react'
import { ControlButton } from '../../design'
import type { SceneLoadingState } from '../../engine/protocol'
import { keyHintFor, useBindingsSnapshot } from '../../lib/bindingLabels'
import { LOADING_TIPS, TIP_ROTATE_MS } from './loadingTips'
import styles from './SceneLoadingOverlay.module.css'

function TipBody({ body, emoteKey }: { body: string; emoteKey: string }): React.JSX.Element {
  const parts = body.split('{Emote}')
  return (
    <p className={styles.tipBody}>
      {parts.map((p, i) => (
        <span key={i}>
          {i > 0 && <kbd className={styles.key}>{emoteKey}</kbd>}
          {p}
        </span>
      ))}
    </p>
  )
}

function TipsCarousel(): React.JSX.Element {
  const [index, setIndex] = useState(0)
  const snap = useBindingsSnapshot()
  const emoteKey = keyHintFor(snap, 'Emote') ?? 'B'
  // Any manual move restarts the rotation clock, like Unity's RotateTipsOverTimeAsync restart.
  useEffect(() => {
    const t = setTimeout(() => setIndex((i) => (i + 1) % LOADING_TIPS.length), TIP_ROTATE_MS)
    return () => clearTimeout(t)
  }, [index])
  const go = (i: number): void => setIndex((i + LOADING_TIPS.length) % LOADING_TIPS.length)
  const tip = LOADING_TIPS[index]

  return (
    <section className={styles.tips} aria-roledescription="carousel" aria-label="Tips">
      <ControlButton className={styles.arrow} shape="circle" variant="solid" aria-label="Previous tip" onClick={() => go(index - 1)}>
        ‹
      </ControlButton>
      <div className={styles.tip} key={index}>
        <img className={styles.tipImage} src={tip.image} alt={tip.title} draggable={false} />
        <div className={styles.tipText}>
          <h2 className={styles.tipTitle}>{tip.title}</h2>
          <TipBody body={tip.body} emoteKey={emoteKey} />
          <div className={styles.dots} role="tablist" aria-label="Choose a tip">
            {LOADING_TIPS.map((t, i) => (
              <button
                key={t.title}
                type="button"
                role="tab"
                aria-label={t.title}
                aria-selected={i === index}
                className={`${styles.dot} ${i === index ? styles.dotActive : ''}`.trim()}
                onClick={() => go(i)}
              />
            ))}
          </div>
        </div>
      </div>
      <ControlButton className={styles.arrow} shape="circle" variant="solid" aria-label="Next tip" onClick={() => go(index + 1)}>
        ›
      </ControlButton>
    </section>
  )
}

export function SceneLoadingOverlay({
  scene,
  travellingTo = null
}: {
  scene: SceneLoadingState | null
  /** A HUD travel is waiting on the engine: name the destination, not the scene being left. */
  travellingTo?: string | null
}): React.JSX.Element {
  // Track the peak pending-asset count to render a sensible progress bar.
  const peak = useRef(0)
  const pending = scene?.pendingAssets ?? null
  if (pending != null && pending > peak.current) peak.current = pending

  const connecting = scene != null && !scene.realmConnected
  const known = !connecting && pending != null && peak.current > 0
  const percent = known ? Math.round((1 - pending / peak.current) * 100) : null
  const status = connecting ? 'RECONNECTING…' : percent != null ? `LOADING ${percent}%` : 'LOADING'

  return (
    <div className={styles.root}>
      <header className={styles.bar}>
        <span className={styles.brand}>
          <span className={styles.brandLogo} />
          Decentraland
        </span>
        <span className={styles.status} role="status">{travellingTo != null ? `TRAVELLING TO ${travellingTo}` : status}</span>
      </header>
      <div className={styles.track}>
        <div
          className={`${styles.fill}${percent != null ? '' : ' ' + styles.indeterminate}`}
          style={percent != null ? { width: `${percent}%` } : undefined}
        />
      </div>
      <TipsCarousel />
    </div>
  )
}
