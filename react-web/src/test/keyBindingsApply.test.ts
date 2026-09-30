// applyCapture: plain inputs replace one chip of one action; analog inputs bind the pushed
// direction to the captured action and the opposite direction to its opposite-pair partner
// (push orients the axis — inverted look is capturing stick-up on Camera Down). Same-axis
// entries on the pair are replaced; other axes and perpendicular siblings are untouched.
import { describe, it, expect } from 'vitest'
import type { BindingEntry } from '../engine/protocol'
import { applyCapture } from '../features/settings/KeyBindingsTab'

const row = (table: BindingEntry[], name: string): string[] | undefined =>
  table.find(([a]) => ('System' in a ? a.System : a.Scene) === name)?.[1]

describe('applyCapture', () => {
  it('a key replaces the clicked chip of just that action', () => {
    const table: BindingEntry[] = [[{ System: 'Places' }, ['KeyZ']]]
    const next = applyCapture(table, { System: 'Places' }, 0, 'KeyX')
    expect(row(next, 'Places')).toEqual(['KeyX'])
  })

  it('a key appends when the add-chip (index === length) was used, deduped', () => {
    const table: BindingEntry[] = [[{ System: 'Places' }, ['KeyZ']]]
    expect(row(applyCapture(table, { System: 'Places' }, 1, 'KeyX'), 'Places')).toEqual(['KeyZ', 'KeyX'])
    expect(row(applyCapture(table, { System: 'Places' }, 1, 'KeyZ'), 'Places')).toEqual(['KeyZ'])
  })

  it('an analog capture binds the pushed direction and mirrors its opposite partner', () => {
    const table: BindingEntry[] = [
      [{ System: 'CameraUp' }, ['ArrowUp']],
      [{ System: 'CameraDown' }, ['ArrowDown']],
      [{ System: 'CameraLeft' }, ['ArrowLeft']],
      [{ System: 'CameraRight' }, ['ArrowRight']]
    ]
    const next = applyCapture(table, { System: 'CameraUp' }, 1, 'GamepadRight Up')
    // keys on other axes kept; perpendicular siblings untouched (stick-splitting)
    expect(row(next, 'CameraUp')).toEqual(['ArrowUp', 'GamepadRight Up'])
    expect(row(next, 'CameraDown')).toEqual(['ArrowDown', 'GamepadRight Down'])
    expect(row(next, 'CameraLeft')).toEqual(['ArrowLeft'])
    expect(row(next, 'CameraRight')).toEqual(['ArrowRight'])
  })

  it('capturing stick-up on the DOWN action inverts the axis', () => {
    const table: BindingEntry[] = [
      [{ System: 'CameraUp' }, []],
      [{ System: 'CameraDown' }, []]
    ]
    const next = applyCapture(table, { System: 'CameraDown' }, 0, 'GamepadRight Up')
    expect(row(next, 'CameraDown')).toEqual(['GamepadRight Up'])
    expect(row(next, 'CameraUp')).toEqual(['GamepadRight Down'])
  })

  it('re-capturing the same axis replaces rather than duplicates', () => {
    const table: BindingEntry[] = [
      [{ System: 'PointerUp' }, ['GamepadRight Up']],
      [{ System: 'PointerDown' }, ['GamepadRight Down']]
    ]
    const next = applyCapture(table, { System: 'PointerUp' }, 0, 'GamepadRight Down')
    expect(row(next, 'PointerUp')).toEqual(['GamepadRight Down'])
    expect(row(next, 'PointerDown')).toEqual(['GamepadRight Up'])
  })

  it('the zoom pair mirrors too', () => {
    const table: BindingEntry[] = [
      [{ System: 'CameraZoomIn' }, ['MouseWheel Up']],
      [{ System: 'CameraZoomOut' }, ['MouseWheel Down']]
    ]
    const next = applyCapture(table, { System: 'CameraZoomIn' }, 1, 'GamepadRightTrigger Up')
    expect(row(next, 'CameraZoomIn')).toEqual(['MouseWheel Up', 'GamepadRightTrigger Up'])
    expect(row(next, 'CameraZoomOut')).toEqual(['MouseWheel Down', 'GamepadRightTrigger Down'])
  })

  it('camera roll is a pair: analog capture mirrors the other roll', () => {
    const table: BindingEntry[] = [
      [{ System: 'RollLeft' }, []],
      [{ System: 'RollRight' }, []]
    ]
    const next = applyCapture(table, { System: 'RollLeft' }, 0, 'GamepadLeftTrigger Up')
    expect(row(next, 'RollLeft')).toEqual(['GamepadLeftTrigger Up'])
    expect(row(next, 'RollRight')).toEqual(['GamepadLeftTrigger Down'])
  })

  it('capturing a wheel direction onto a Scroll action is ignored (the wheel role is fixed)', () => {
    const table: BindingEntry[] = [
      [{ System: 'ScrollUp' }, ['MouseWheel Up']],
      [{ System: 'ScrollDown' }, ['MouseWheel Down']]
    ]
    expect(applyCapture(table, { System: 'ScrollUp' }, 1, 'MouseWheel Down')).toEqual(table)
  })

  it('an analog capture on a NON-pair action stays a plain per-chip bind', () => {
    const table: BindingEntry[] = [[{ System: 'Places' }, ['KeyZ']]]
    const next = applyCapture(table, { System: 'Places' }, 1, 'MouseWheel Down')
    expect(row(next, 'Places')).toEqual(['KeyZ', 'MouseWheel Down'])
  })
})
