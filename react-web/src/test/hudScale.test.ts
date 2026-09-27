import { describe, it, expect, beforeAll } from 'vitest'
import { getHudScale, installHudScale, subscribeHudScale } from '../lib/hudScale'

const cssScale = (): string => document.documentElement.style.getPropertyValue('--ui-scale')

function resizeTo(height: number, width = height * 3): void {
  Object.defineProperty(window, 'innerWidth', { value: width, configurable: true })
  Object.defineProperty(window, 'innerHeight', { value: height, configurable: true })
  window.dispatchEvent(new Event('resize'))
}

beforeAll(() => installHudScale())

// DOMAIN: hudScale — `--ui-scale` follows the viewport against a 1920x1080 reference (smaller ratio wins), clamped, and the
// engine cutouts (minimap, avatar preview) re-measure off it. The ordering below is the whole
// point: the HUD is scaled with a CSS transform, so a subscriber that measures the DOM has to
// run after the property is written or it reports a rect from the pre-resize layout.
describe('hud scale', () => {
  it('follows the viewport height, clamped', () => {
    resizeTo(1080)
    expect(cssScale()).toBe('1.000')
    resizeTo(810)
    expect(cssScale()).toBe('0.750')
    resizeTo(400) // floor
    expect(cssScale()).toBe('0.600')
    resizeTo(2160) // ceiling
    expect(cssScale()).toBe('1.300')
    expect(getHudScale()).toBe(1.3)
  })

  it('uses the smaller of the width and height ratios, so the canvas expands along the longer axis', () => {
    resizeTo(960, 1536) // 16:10: width-bound (height alone would give 0.889)
    expect(cssScale()).toBe('0.800')
    resizeTo(1200, 1920) // 1920x1200: one canvas px per CSS px, extra height
    expect(cssScale()).toBe('1.000')
    resizeTo(1080, 1280) // narrow window
    expect(cssScale()).toBe('0.667')
  })

  it('writes --ui-scale before waking subscribers, in the same task', () => {
    resizeTo(1080)
    const seen: string[] = []
    const off = subscribeHudScale(() => seen.push(cssScale()))
    resizeTo(648)
    off()
    expect(seen).toEqual(['0.600'])
  })

  it('does not wake subscribers when the scale is unchanged', () => {
    resizeTo(1080)
    let woken = 0
    const off = subscribeHudScale(() => woken++)
    resizeTo(1080) // same height: no change, no wake
    resizeTo(3000) // 1.000 -> clamped 1.300: one wake
    off()
    expect(woken).toBe(1)
  })
})
