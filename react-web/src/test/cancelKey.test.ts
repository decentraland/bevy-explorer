// cancelKeyCodes/isCancelKey: Escape stands in only until the engine's binding table
// arrives; once it's here the Cancel binding is authoritative — rebound follows the new
// key, and UNBOUND means no key cancels (matching the engine, which resolves nothing).
import { describe, it, expect, afterEach } from 'vitest'
import { cancelKeyCodes, isCancelKey, setBindingsSnapshot } from '../lib/bindingLabels'

const key = (init: { key?: string; code?: string }): Pick<KeyboardEvent, 'key' | 'code'> => ({
  key: init.key ?? '',
  code: init.code ?? ''
})

afterEach(() => setBindingsSnapshot([])) // module store leaks across tests

describe('cancelKeyCodes', () => {
  it('falls back to Escape only while no table has arrived', () => {
    expect(cancelKeyCodes()).toEqual(['Escape'])
    expect(isCancelKey(key({ key: 'Escape', code: 'Escape' }))).toBe(true)
  })

  it('follows a rebound Cancel: the new key cancels, Escape no longer does', () => {
    setBindingsSnapshot([[{ System: 'Cancel' }, ['KeyR']]])
    expect(cancelKeyCodes()).toEqual(['KeyR'])
    expect(isCancelKey(key({ key: 'r', code: 'KeyR' }))).toBe(true)
    expect(isCancelKey(key({ key: 'Escape', code: 'Escape' }))).toBe(false)
  })

  it('an unbound Cancel means NO key cancels (chat blur, panel close all stand down)', () => {
    setBindingsSnapshot([
      [{ System: 'Cancel' }, []],
      [{ System: 'Map' }, ['KeyM']] // the table itself is present
    ])
    expect(cancelKeyCodes()).toEqual([])
    expect(isCancelKey(key({ key: 'Escape', code: 'Escape' }))).toBe(false)
  })

  it('non-key Cancel bindings (gamepad) are not cancel KEYS', () => {
    setBindingsSnapshot([[{ System: 'Cancel' }, ['Gamepad Select']]])
    expect(cancelKeyCodes()).toEqual([])
    expect(isCancelKey(key({ key: 'Escape', code: 'Escape' }))).toBe(false)
  })
})
