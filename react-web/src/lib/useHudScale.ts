// Keeps `--ui-scale` in sync with the viewport, mirroring Unity's CanvasScaler
// ("Scale With Screen Size", 1080 reference height). Because window.innerHeight is
// already in logical px (device px ÷ devicePixelRatio), this is DPI-correct: a
// Retina 3024×1890 framebuffer is ~945 logical tall → scale ≈ 0.87, matching how
// Unity scales its HUD on the same display. Flexible across any resolution.

import { useEffect } from 'react'

const REFERENCE_HEIGHT = 1080
const MIN = 0.6
const MAX = 1.3

export function useHudScale(): void {
  useEffect(() => {
    const apply = (): void => {
      const scale = Math.min(MAX, Math.max(MIN, window.innerHeight / REFERENCE_HEIGHT))
      document.documentElement.style.setProperty('--ui-scale', scale.toFixed(3))
    }
    apply()
    window.addEventListener('resize', apply)
    return () => window.removeEventListener('resize', apply)
  }, [])
}
