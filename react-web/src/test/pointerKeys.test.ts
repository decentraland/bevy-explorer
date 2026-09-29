import { describe, it, expect } from 'vitest'
import { proximityKey } from '../engine/pointerKeys'

const sit = [{ button: 0, text: 'Sit', enabled: true }]

describe('proximity change key (what the bridge sends on)', () => {
  it('ignores sub-pixel movement', () => {
    expect(proximityKey([{ id: 1, x: 10.2, y: 20.4, actions: sit }])).toBe(proximityKey([{ id: 1, x: 9.6, y: 19.8, actions: sit }]))
  })

  it('changes on a whole-pixel move, a new tip, a gone tip or different actions', () => {
    const base = proximityKey([{ id: 1, x: 10, y: 20, actions: sit }])
    expect(proximityKey([{ id: 1, x: 12, y: 20, actions: sit }])).not.toBe(base)
    expect(proximityKey([{ id: 1, x: 10, y: 20, actions: sit }, { id: 2, x: 0, y: 0, actions: sit }])).not.toBe(base)
    expect(proximityKey([])).not.toBe(base)
    expect(proximityKey([{ id: 1, x: 10, y: 20, actions: [{ button: 1, text: 'Open', enabled: true }] }])).not.toBe(base)
  })
})
