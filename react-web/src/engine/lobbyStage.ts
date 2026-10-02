// The lobby stage's backdrop, floor shade and vignette, shared by the bridge (which draws the stage)
// and the page (which draws a stand-in until the stage is ready), so the two can't drift apart.
// Measured against a capture of the parity stage at 1920×1200.

/** The backdrop image's aspect, and its height and top as fractions of the screen height (the
 *  stage camera's vertical field of view is fixed). Centred horizontally. */
export const BACKDROP_ASPECT = 1595 / 986
export const BACKDROP_HEIGHT = 1.033
export const BACKDROP_TOP = -0.17

export function backdropRect(width: number, height: number): { left: number; top: number; width: number; height: number } {
  const h = Math.max(height * BACKDROP_HEIGHT, width / BACKDROP_ASPECT)
  const w = h * BACKDROP_ASPECT
  return { left: (width - w) / 2, top: height * BACKDROP_TOP, width: w, height: h }
}

/** The floor's colour (0–255) and how much it covers the backdrop, by screen height. */
export const FLOOR = { r: 37, g: 5, b: 3 }
export const FLOOR_SHADE_STOPS: ReadonlyArray<readonly [number, number]> = [
  [0.54, 0],
  [0.62, 0.23],
  [0.71, 0.61],
  [0.8, 0.73],
  [0.9, 0.85],
  [1, 1]
]

export function floorShadeAt(y: number): number {
  const k = FLOOR_SHADE_STOPS.findIndex(([at]) => at >= y)
  if (k <= 0) return k === 0 ? FLOOR_SHADE_STOPS[0][1] : 1
  const [y0, a0] = FLOOR_SHADE_STOPS[k - 1]
  const [y1, a1] = FLOOR_SHADE_STOPS[k]
  return a0 + ((a1 - a0) * (y - y0)) / (y1 - y0)
}

export function floorShadeGradient(): string {
  const { r, g, b } = FLOOR
  const stops = FLOOR_SHADE_STOPS.map(([at, a]) => `rgba(${r}, ${g}, ${b}, ${a}) ${at * 100}%`)
  return `linear-gradient(to bottom, ${stops.join(', ')})`
}
