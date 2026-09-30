import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { DateField } from '../design/DateField'

// COMPONENT: a date built from our own Selects. `input type="date"` opens a Chromium popup widget,
// and the HUD renders offscreen — CEF paints popups as a surface the engine does not composite, so
// the picker replaced the entire HUD image. Nothing in the HUD may open a native popup.

const pick = async (label: string, option: string): Promise<void> => {
  await userEvent.click(screen.getByRole('button', { name: label }))
  await userEvent.click(screen.getByRole('option', { name: option }))
}

describe('DateField', () => {
  it('shows the parts of the date it was given', () => {
    render(<DateField label="Birth Date" value="2003-02-01" onChange={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'Birth Date day' })).toHaveTextContent('1')
    expect(screen.getByRole('button', { name: 'Birth Date month' })).toHaveTextContent('February')
    expect(screen.getByRole('button', { name: 'Birth Date year' })).toHaveTextContent('2003')
  })

  it('reports nothing until every part is chosen, then the whole date', async () => {
    const onChange = vi.fn()
    render(<DateField label="Birth Date" value="" onChange={onChange} />)

    await pick('Birth Date year', '2003')
    expect(onChange).toHaveBeenLastCalledWith('')
    await pick('Birth Date month', 'February')
    expect(onChange).toHaveBeenLastCalledWith('')
    await pick('Birth Date day', '1')
    expect(onChange).toHaveBeenLastCalledWith('2003-02-01')
  })

  it('never offers — or keeps — a day the month does not have', async () => {
    const onChange = vi.fn()
    render(<DateField label="Birth Date" value="2003-01-31" onChange={onChange} />)
    await pick('Birth Date month', 'February')
    // The 31st becomes the 28th rather than emitting a date that does not exist.
    expect(onChange).toHaveBeenLastCalledWith('2003-02-28')
    expect(screen.queryByRole('option', { name: '31' })).toBeNull()
  })

  it('offers the 29th in a leap year', async () => {
    render(<DateField label="Birth Date" value="2004-02-01" onChange={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'Birth Date day' }))
    expect(screen.getByRole('option', { name: '29' })).toBeTruthy()
  })

  it('clears back to nothing', async () => {
    const onChange = vi.fn()
    render(<DateField label="Birth Date" value="2003-02-01" onChange={onChange} />)
    await pick('Birth Date year', 'Year')
    expect(onChange).toHaveBeenLastCalledWith('')
  })
})
