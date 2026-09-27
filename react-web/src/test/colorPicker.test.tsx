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

// A COLOR button opening presets and HSV bars.
describe('ColorPicker', () => {
  const presets = ['#1c1c1c', '#3c210b', '#ffbe28']
  const open = (value: string, onChange = vi.fn()) => {
    render(<ColorPicker label="Hair color" value={value} presets={presets} onChange={onChange} />)
    fireEvent.click(screen.getByRole('button', { name: 'Hair color' }))
    return onChange
  }

  it('is a COLOR button until opened', () => {
    render(<ColorPicker label="Hair color" value="#3c210b" presets={presets} onChange={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'Hair color' })).toHaveTextContent('COLOR')
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('highlights the preset matching the current colour and picks another', () => {
    const onChange = open('#3c210b')
    expect(screen.getByRole('radio', { name: '#3c210b' })).toBeChecked()
    fireEvent.click(screen.getByRole('radio', { name: '#ffbe28' }))
    expect(onChange).toHaveBeenCalledWith('#ffbe28')
    expect(screen.getByRole('radio', { name: '#ffbe28' })).toBeChecked()
  })

  it('moving a bar clears the preset highlight', () => {
    open('#3c210b')
    fireEvent.change(screen.getByRole('slider', { name: 'Brightness' }), { target: { value: '0.5' } })
    expect(screen.getByRole('radio', { name: '#3c210b' })).not.toBeChecked()
  })

  it('the arrows step a bar by 0.1 and stop at its ends', () => {
    const onChange = open('#ffbe28')
    fireEvent.click(screen.getByRole('button', { name: 'Decrease Brightness' }))
    expect(hexToHsv(onChange.mock.calls[0][0] as string).v).toBeCloseTo(0.9, 2)
    expect(screen.getByRole('button', { name: 'Increase Brightness' })).not.toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: 'Increase Brightness' }))
    expect(screen.getByRole('button', { name: 'Increase Brightness' })).toBeDisabled()
  })

  it('keeps the hue when brightness goes to zero and back', () => {
    const onChange = open('#ffbe28')
    const brightness = screen.getByRole('slider', { name: 'Brightness' })
    fireEvent.change(brightness, { target: { value: '0' } })
    fireEvent.change(brightness, { target: { value: '1' } })
    expect(hexToHsv(onChange.mock.calls.at(-1)?.[0] as string).h).toBeCloseTo(hexToHsv('#ffbe28').h, 0)
  })

  it('closes on a click outside', () => {
    open('#3c210b')
    fireEvent.mouseDown(document.body)
    expect(screen.queryByRole('dialog')).toBeNull()
  })
})
