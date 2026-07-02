// Live FPS measurement for the perf overlay.
//
// PAGE fps = the React/page main-thread frame rate (requestAnimationFrame). Because the
// engine runs in a SAME-ORIGIN iframe, it shares this main thread — so if React re-renders
// starve the frame budget, this number drops. That's the "is the HUD hurting perf?" signal.
//
// ENGINE fps = the bevy render loop's rate. On WEB it's read by wrapping the engine iframe's
// per-frame `window.__engineHeartbeat()` (deploy/web/index.html). On NATIVE there is no iframe —
// the native overlay pushes the engine's measured fps to `window.__nativeEngineFps` (see
// src/react_hud.rs), which we read directly. null when there's no engine or it hasn't booted yet.

import { useEffect, useState } from 'react'

export interface FpsStats {
  /** Page main-thread fps (rAF). */
  page: number
  /** Engine render fps (heartbeat), or null if no engine. */
  engine: number | null
  /** Average page frame time, ms. */
  ms: number
}

type HeartbeatWindow = Window & {
  __engineHeartbeat?: (...a: unknown[]) => unknown
  // Native overlay pushes bevy's measured fps here (no engine iframe on native).
  __nativeEngineFps?: number
}

export function useFps(enabled: boolean): FpsStats {
  const [stats, setStats] = useState<FpsStats>({ page: 0, engine: null, ms: 0 })

  useEffect(() => {
    if (!enabled) return
    let raf = 0
    let frames = 0
    let engineFrames = 0
    let hookedWin: HeartbeatWindow | null = null
    let origBeat: ((...a: unknown[]) => unknown) | undefined
    let windowStart = performance.now()
    let lastTs = windowStart
    let msAccum = 0

    // Wrap the engine iframe's per-frame heartbeat so we can count its frames.
    const hookEngine = (): void => {
      const iframe = document.querySelector<HTMLIFrameElement>('iframe[title="Decentraland engine"]')
      const w = iframe?.contentWindow as HeartbeatWindow | null | undefined
      if (!w || w === hookedWin || typeof w.__engineHeartbeat !== 'function') return
      origBeat = w.__engineHeartbeat
      w.__engineHeartbeat = function (this: unknown, ...args: unknown[]) {
        engineFrames++
        return origBeat?.apply(this, args)
      }
      hookedWin = w
    }

    const loop = (t: number): void => {
      frames++
      msAccum += t - lastTs
      lastTs = t
      hookEngine()
      if (t - windowStart >= 500) {
        const secs = (t - windowStart) / 1000
        // Native pushes the real engine fps here; otherwise count the iframe heartbeat (web).
        const nativeEngine = (window as HeartbeatWindow).__nativeEngineFps
        const engine =
          typeof nativeEngine === 'number'
            ? Math.round(nativeEngine)
            : hookedWin
              ? Math.round(engineFrames / secs)
              : null
        setStats({
          page: Math.round(frames / secs),
          engine,
          ms: Number((msAccum / frames).toFixed(1))
        })
        frames = 0
        engineFrames = 0
        msAccum = 0
        windowStart = t
      }
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)

    return () => {
      cancelAnimationFrame(raf)
      // Restore the engine's original heartbeat.
      if (hookedWin && origBeat) hookedWin.__engineHeartbeat = origBeat
    }
  }, [enabled])

  return stats
}
