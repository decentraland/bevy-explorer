import { describe, it, expect } from 'vitest'
import { preferUp } from '../design/Select'

// A dropdown at the bottom of a scrolling panel is cut off by that panel long before it reaches the
// bottom of the screen — the community create modal clipped its membership list and grew a
// scrollbar instead. The direction is decided against the clipping box, not the viewport.

describe('which way a Select opens', () => {
  it('opens downwards when there is room below', () => {
    expect(preferUp({ above: 300, below: 300, list: 260 })).toBe(false)
  })

  it('opens upwards when the room below cannot hold the list and there is more above', () => {
    expect(preferUp({ above: 320, below: 40, list: 260 })).toBe(true)
  })

  it('stays downwards when neither side fits but below is the roomier one', () => {
    // Flipping into an even tighter space buys nothing; the list scrolls internally either way.
    expect(preferUp({ above: 30, below: 90, list: 260 })).toBe(false)
  })

  it('needs only enough room, not spare room', () => {
    expect(preferUp({ above: 1000, below: 260, list: 260 })).toBe(false)
  })
})
