// Live FPS measurement for the perf overlay.
//
// PAGE fps = the React/page main-thread frame rate (requestAnimationFrame). Because the
// engine runs in THIS document, sharing the main thread — so if React re-renders
// starve the frame budget, this number drops. That's the "is the HUD hurting perf?" signal.
//
// ENGINE fps = the bevy render loop's rate, read by wrapping the engine's per-frame
// `window.__engineHeartbeat()` (deploy/web/engine/boot.js; the Rust loop calls it every frame).
// On NATIVE the overlay pushes bevy's measured fps to `window.__nativeEngineFps` instead.
// null when there's no engine (mock mode) or it hasn't booted yet.

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
  // Native overlay pushes bevy's measured fps here (no engine on this document's rAF loop).
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

    // Wrap the engine's per-frame heartbeat so we can count its frames. Same-document engine
    // (no iframe): boot.js installs __engineHeartbeat on THIS window once the module loads —
    // re-check each frame until it appears.
    const hookEngine = (): void => {
      const w = window as HeartbeatWindow
      if (w === hookedWin || typeof w.__engineHeartbeat !== 'function') return
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
        setStats({
          page: Math.round(frames / secs),
          // Native pushes the real engine fps (see src/react_hud.rs); otherwise count the
          // same-document heartbeat (web).
          engine:
            typeof (window as HeartbeatWindow).__nativeEngineFps === 'number'
              ? Math.round((window as HeartbeatWindow).__nativeEngineFps!)
              : hookedWin
                ? Math.round(engineFrames / secs)
                : null,
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
