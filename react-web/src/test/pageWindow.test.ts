import { describe, it, expect } from 'vitest'
import { pageWindow } from '../features/backpack/BackpackPage'

// pageWindow is a sliding window of up to 5 consecutive 0-based page indices (bevy-ui-scene style);
// the UI shows each +1. The current page stays centered once past the first half, clamped at ends.
describe('pageWindow (sliding window pager)', () => {
  it('shows every page when there are 5 or fewer', () => {
    expect(pageWindow(0, 1)).toEqual([0])
    expect(pageWindow(2, 5)).toEqual([0, 1, 2, 3, 4])
    expect(pageWindow(3, 4)).toEqual([0, 1, 2, 3])
  })

  it('keeps the window anchored at the start for the first pages (1 2 3 4 5)', () => {
    expect(pageWindow(0, 20)).toEqual([0, 1, 2, 3, 4]) // page 1
    expect(pageWindow(1, 20)).toEqual([0, 1, 2, 3, 4]) // page 2
    expect(pageWindow(2, 20)).toEqual([0, 1, 2, 3, 4]) // page 3
  })

  it('centers the current page in the middle (… slides)', () => {
    expect(pageWindow(3, 20)).toEqual([1, 2, 3, 4, 5]) // page 4 → centered
    expect(pageWindow(9, 20)).toEqual([7, 8, 9, 10, 11]) // page 10 → centered
  })

  it('clamps the window at the end for the last pages (16 17 18 19 20)', () => {
    expect(pageWindow(18, 20)).toEqual([15, 16, 17, 18, 19]) // page 19
    expect(pageWindow(19, 20)).toEqual([15, 16, 17, 18, 19]) // page 20
  })
})
