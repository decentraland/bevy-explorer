import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { FpsMeter } from '../features/debug/FpsMeter'

describe('FpsMeter', () => {
  it('renders a page fps readout and hides engine fps when no engine is present', () => {
    render(<FpsMeter />)
    expect(screen.getByText('fps')).toBeInTheDocument()
    // No engine iframe in jsdom → engine reading is omitted.
    expect(screen.queryByText(/engine/)).not.toBeInTheDocument()
  })
})
