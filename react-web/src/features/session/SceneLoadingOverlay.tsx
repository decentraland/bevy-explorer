// React scene-asset loading screen — replaces the engine's in-world loading UI.
// Driven by the bridge scene's getSceneLoadingUIStream relay. Layout follows unity-explorer's
// SceneLoadingScreenView: a top bar with LOADING N% over a progress line, and a tips carousel.

import { useEffect, useMemo, useState } from 'react'
import type { SceneLoadingState } from '../../engine/protocol'
import { keyHintFor, useBindingsSnapshot, type BindingsSnapshot } from '../../lib/bindingLabels'
import { launchCount } from '../../lib/launchCount'
import { bugReportUrl } from '../../lib/bugReport'
import { flagEnabled, useFeatureFlags } from '../../lib/featureFlags'
import { Icon } from '../../design'
import logoIcon from '../../assets/loading/logo-icon.webp'
import wordmark from '../../assets/loading/wordmark.webp'
import { TIP_ROTATE_MS, tipsFor, type LoadingTip } from './loadingTips'
import prevTip from '../../assets/loading/prev-tip.webp'
import nextTip from '../../assets/loading/next-tip.webp'
import styles from './SceneLoadingOverlay.module.css'

const LAST_TIP_KEY = 'loadingLastTip'
const FADE_MS = 300

function readLastTip(): number {
  try {
    return Number(localStorage.getItem(LAST_TIP_KEY) ?? -1)
  } catch {
    return -1
  }
}

function TipBody({ body, emoteKey }: { body: string; emoteKey: string }): React.JSX.Element {
  const parts = body.split('{Emote}')
  return (
    <p className={styles.tipBody}>
      {parts.map((p, i) => (
        <span key={i}>
          {i > 0 && <span className={styles.accent}>{emoteKey}</span>}
          {p}
        </span>
      ))}
    </p>
  )
}

function TipAction({ action, snap }: { action: NonNullable<LoadingTip['action']>; snap: BindingsSnapshot }): React.JSX.Element {
  return (
    <div className={styles.action}>
      {action.icon != null && (
        <span className={styles.actionIcon}>
          {typeof action.icon === 'string' ? (
            <Icon name={action.icon} size={action.iconSize ?? 42} />
          ) : (
            <img src={action.icon.src} alt="" width={action.iconSize ?? 42} height={action.iconSize ?? 42} />
          )}
        </span>
      )}
      <span className={styles.actionText}>
        {action.parts.map((part, i) =>
          typeof part === 'string' ? (
            <span key={i}>{part}</span>
          ) : 'icon' in part ? (
            <img key={i} className={styles.actionInlineIcon} src={part.icon} alt="" />
          ) : (
            <span key={i} className={styles.accent}>
              {(part.binding != null ? keyHintFor(snap, part.binding) : undefined) ?? part.accent}
            </span>
          )
        )}
      </span>
    </div>
  )
}

function TipsCarousel(): React.JSX.Element {
  const flags = useFeatureFlags()
  const tips = useMemo(() => tipsFor(flags, launchCount()), [flags])
  // Each loading screen picks up after the last tip shown.
  const [index, setIndex] = useState(() => (readLastTip() + 1) % tips.length)
  const [shown, setShown] = useState(index)
  const [epoch, setEpoch] = useState(0)
  const snap = useBindingsSnapshot()
  const emoteKey = keyHintFor(snap, 'Emote') ?? 'B'
  const count = tips.length
  const current = index % count

  useEffect(() => {
    const t = setInterval(() => setIndex((i) => (i + 1) % count), TIP_ROTATE_MS)
    return () => clearInterval(t)
  }, [epoch, count])
  // Fade the old tip out, then the new one in.
  useEffect(() => {
    try {
      localStorage.setItem(LAST_TIP_KEY, String(current))
    } catch {}
    if (current === shown) return
    const t = setTimeout(() => setShown(current), FADE_MS)
    return () => clearTimeout(t)
  }, [current, shown])

  const step = (d: number): void => {
    setIndex((i) => (i + d + count) % count)
    setEpoch((e) => e + 1)
  }
  const tip = tips[shown % count]

  return (
    <section className={styles.tips} aria-roledescription="carousel" aria-label="Tips">
      <div className={styles.tipsBox}>
        <div className={`${styles.tip} ${current !== shown ? styles.tipOut : ''}`.trim()} key={tip.key}>
          <img className={styles.tipImage} src={tip.image} alt={tip.title} draggable={false} />
          <h2 className={styles.tipTitle}>{tip.title}</h2>
          <TipBody body={tip.body} emoteKey={emoteKey} />
          {tip.action != null && <TipAction action={tip.action} snap={snap} />}
        </div>
      </div>
      <div className={styles.dots} role="tablist" aria-label="Choose a tip">
          {tips.map((t, i) => (
            <button
              key={t.key}
              type="button"
              role="tab"
              aria-label={t.title}
              aria-selected={i === current}
              className={`${styles.dot} ${i === current ? styles.dotActive : ''}`.trim()}
              onClick={() => setIndex(i)}
            />
          ))}
      </div>
      <button type="button" className={`${styles.arrow} ${styles.arrowPrev}`} aria-label="Previous tip" onClick={() => step(-1)}>
        <img src={prevTip} alt="" />
      </button>
      <button type="button" className={`${styles.arrow} ${styles.arrowNext}`} aria-label="Next tip" onClick={() => step(1)}>
        <img src={nextTip} alt="" />
      </button>
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
            <Icon name="bug" size={24} />
          </button>
        )}
      </div>
    </div>
  )
}
