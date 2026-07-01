// Keeps `--ui-scale` in sync with the viewport, mirroring Unity's CanvasScaler
// ("Scale With Screen Size", 1080 reference height). On web, window.innerHeight is already in
// logical px (device px ÷ devicePixelRatio), so innerHeight/1080 is DPI-correct: a Retina
// 3024×1890 framebuffer is ~945 logical tall → scale ≈ 0.87.
//
// NATIVE: the wry/WKWebView overlay does NOT report a reliable logical innerHeight (and fires no
// dependable resize), which made full-screen pages render too big. So the native overlay pushes
// bevy's authoritative logical window height to `window.__nativeUiHeight` (see src/react_hud.rs);
// we prefer it and poll for changes since there's no resize event to rely on.

import { useEffect } from 'react'

const REFERENCE_HEIGHT = 1080
const MIN = 0.6
const MAX = 1.3

type ScaleWindow = Window & { __nativeUiHeight?: number }
const IS_NATIVE = new URLSearchParams(location.search).get('native') === '1'

export function useHudScale(): void {
  useEffect(() => {
    let last = -1
    const apply = (): void => {
      const nh = (window as ScaleWindow).__nativeUiHeight
      const height = typeof nh === 'number' && nh > 0 ? nh : window.innerHeight
      const scale = Math.min(MAX, Math.max(MIN, height / REFERENCE_HEIGHT))
      if (scale === last) return
      last = scale
      document.documentElement.style.setProperty('--ui-scale', scale.toFixed(3))
    }
    apply()
    window.addEventListener('resize', apply)
    // Native pushes __nativeUiHeight but has no reliable resize event — poll for changes.
    const poll = IS_NATIVE ? window.setInterval(apply, 500) : 0
    return () => {
      window.removeEventListener('resize', apply)
      if (poll) window.clearInterval(poll)
    }
  }, [])
}
