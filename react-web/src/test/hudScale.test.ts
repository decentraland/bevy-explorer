import { describe, it, expect, beforeAll } from 'vitest'
import { getHudScale, installHudScale, subscribeHudScale } from '../lib/hudScale'

const cssScale = (): string => document.documentElement.style.getPropertyValue('--ui-scale')

function resizeTo(height: number): void {
  Object.defineProperty(window, 'innerHeight', { value: height, configurable: true })
  window.dispatchEvent(new Event('resize'))
}

beforeAll(() => installHudScale())

// DOMAIN: hudScale — `--ui-scale` follows the viewport on Unity's clamped 1080 curve, and the
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
