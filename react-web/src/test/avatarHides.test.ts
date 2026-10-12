import { describe, it, expect } from 'vitest'
import { hiddenBy, itemHides } from '../engine/avatarHides'

const MALE = 'urn:decentraland:off-chain:base-avatars:BaseMale'
const FEMALE = 'urn:decentraland:off-chain:base-avatars:BaseFemale'

describe('itemHides', () => {
  it('joins hides and replaces, and never hides its own category', () => {
    expect(itemHides('hat', { hides: ['hair', 'hat'], replaces: ['helmet'] }, MALE).sort()).toEqual(['hair', 'helmet'])
  })

  it('uses the body shape representation overrides when set', () => {
    const data = { hides: ['hair'], representations: [{ bodyShapes: [FEMALE], overrideHides: ['earring'] }] }
    expect(itemHides('hat', data, FEMALE)).toEqual(['earring'])
    expect(itemHides('hat', data, MALE)).toEqual(['hair'])
  })

  it('upper body hides hands unless it removes that default', () => {
    expect(itemHides('upper_body', {}, MALE)).toEqual(['hands'])
    expect(itemHides('upper_body', { removesDefaultHiding: ['hands'] }, MALE)).toEqual([])
  })

  it('skin hides the whole body', () => {
    expect(itemHides('skin', {}, MALE)).toEqual(expect.arrayContaining(['hair', 'upper_body', 'feet', 'hands']))
  })
})

describe('hiddenBy', () => {
  it('maps each hidden category to the item hiding it', () => {
    const m = hiddenBy([{ category: 'helmet', hides: ['hair', 'hat'] }, { category: 'hair' }, { category: 'hat' }], [])
    expect(m.get('hair')).toBe('helmet')
    expect(m.get('hat')).toBe('helmet')
  })

  it('ignores the hides of an item that is itself hidden', () => {
    // The helmet hides the mask, so the mask's own hides (eyewear) don't apply.
    const m = hiddenBy([{ category: 'helmet', hides: ['mask'] }, { category: 'mask', hides: ['eyewear'] }, { category: 'eyewear' }], [])
    expect(m.has('eyewear')).toBe(false)
  })

  it('never hides a force-rendered category', () => {
    const m = hiddenBy([{ category: 'hat', hides: ['hair'] }, { category: 'hair' }], ['hair'])
    expect(m.has('hair')).toBe(false)
  })
})
