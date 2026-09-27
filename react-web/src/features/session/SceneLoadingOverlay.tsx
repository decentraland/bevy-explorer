// React scene-asset loading screen — replaces the engine's in-world loading UI.
// Driven by the bridge scene's getSceneLoadingUIStream relay. Layout follows unity-explorer's
// SceneLoadingScreenView: a top bar with LOADING N% over a progress line, and a tips carousel.

import { useEffect, useState } from 'react'
import { ControlButton } from '../../design'
import type { SceneLoadingState } from '../../engine/protocol'
import { keyHintFor, useBindingsSnapshot } from '../../lib/bindingLabels'
import { bugReportUrl } from '../../lib/bugReport'
import { flagEnabled, useFeatureFlags } from '../../lib/featureFlags'
import { MaskIcon } from '../../design'
import logoIcon from '../../assets/loading/logo-icon.webp'
import wordmark from '../../assets/loading/wordmark.webp'
import bugIcon from '../../assets/loading/icon-bug.webp'
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
  progress,
  travellingTo = null
}: {
  scene: SceneLoadingState | null
  /** 0–100 across every loading stage (session.loadingProgress). */
  progress: number
  /** A HUD travel is waiting on the engine: name the destination, not the scene being left. */
  travellingTo?: string | null
}): React.JSX.Element {
  const connecting = scene != null && !scene.realmConnected
  const status = connecting ? 'RECONNECTING…' : `LOADING ${progress}%`
  const flags = useFeatureFlags()

  return (
    <div className={styles.root}>
      <div className={styles.pattern} aria-hidden="true">
        <div className={styles.patternX}>
          <div className={styles.patternY} />
        </div>
      </div>
      <div className={styles.canvas}>
        <div className={styles.glow} aria-hidden="true" />
        <header className={styles.bar}>
          <span className={styles.brand}>
            <img className={styles.brandIcon} src={logoIcon} alt="" />
            <img className={styles.brandWordmark} src={wordmark} alt="Decentraland" />
          </span>
          <span className={styles.status} role="status">{travellingTo != null ? `TRAVELLING TO ${travellingTo}` : status}</span>
        </header>
        <div className={styles.track}>
          <div className={styles.fill} style={{ width: `calc(${progress}% + 67px)` }} />
        </div>
        <TipsCarousel />
        {flagEnabled(flags, 'alfa-bug-report') && (
          <button type="button" className={styles.bug} aria-label="Report a bug" onClick={() => window.open(bugReportUrl(), '_blank', 'noopener')}>
            <MaskIcon src={bugIcon} size={24} />
          </button>
        )}
      </div>
    </div>
  )
}
