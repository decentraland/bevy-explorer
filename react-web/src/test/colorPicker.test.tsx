import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { ColorPicker } from '../design'
import { color3ToHex, hexToColor3, hexToHsv, hsvToHex } from '../lib/color'

describe('color conversions', () => {
  it('round-trips hex through HSV', () => {
    for (const hex of ['#ffe4c6', '#20b3f6', '#1c1c1c', '#d4d4d4', '#8c2014']) expect(hsvToHex(hexToHsv(hex))).toBe(hex)
  })

  it('maps engine Color3 (0–1) to hex and back', () => {
    expect(color3ToHex({ r: 1, g: 0.5, b: 0 })).toBe('#ff8000')
    expect(hexToColor3('#ff8000')).toEqual({ r: 1, g: 128 / 255, b: 0 })
  })
})

// Unity ColorPickerView: preset toggles + hue / saturation / value sliders.
describe('ColorPicker', () => {
  const presets = ['#1c1c1c', '#3c210b', '#ffbe28']

  it('marks the current preset and picks another', () => {
    const onChange = vi.fn()
    render(<ColorPicker label="Hair color" value="#3c210b" presets={presets} onChange={onChange} />)
    expect(screen.getByRole('radio', { name: '#3c210b' })).toBeChecked()
    fireEvent.click(screen.getByRole('radio', { name: '#ffbe28' }))
    expect(onChange).toHaveBeenCalledWith('#ffbe28')
  })

  it('the value slider darkens the colour', () => {
    const onChange = vi.fn()
    render(<ColorPicker label="Hair color" value="#ffbe28" presets={presets} onChange={onChange} />)
    fireEvent.change(screen.getByRole('slider', { name: 'Brightness' }), { target: { value: '50' } })
    const [hex] = onChange.mock.calls[0] as [string]
    expect(hexToHsv(hex).v).toBeCloseTo(0.5, 2)
    expect(hexToHsv(hex).h).toBeCloseTo(hexToHsv('#ffbe28').h, 0)
  })
})
