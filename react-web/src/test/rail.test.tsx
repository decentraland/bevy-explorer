import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Rail } from '../design/Rail'

afterEach(() => vi.restoreAllMocks())

describe('Rail pagination', () => {
  it('keeps Next available for the last card of seven and supports returning from the clamped final page', () => {
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(280)
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(864)
    vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockReturnValue(2008)
    const original = HTMLElement.prototype.scrollTo
    HTMLElement.prototype.scrollTo = function (options?: ScrollToOptions | number): void {
      this.scrollLeft = Math.min(typeof options === 'object' ? options.left ?? 0 : options ?? 0, 1144)
      fireEvent.scroll(this)
    }
    try {
      render(<Rail perPage={3} gap={8}>{Array.from({ length: 7 }, (_, i) => <div key={i}>Place {i + 1}</div>)}</Rail>)
      fireEvent.click(screen.getByRole('button', { name: 'Next' }))
      expect(screen.getByRole('button', { name: 'Page 2' })).toHaveAttribute('aria-current', 'true')
      fireEvent.click(screen.getByRole('button', { name: 'Next' }))
      expect(screen.getByRole('button', { name: 'Page 3' })).toHaveAttribute('aria-current', 'true')
      expect(screen.queryByRole('button', { name: 'Next' })).toBeNull()
      fireEvent.click(screen.getByRole('button', { name: 'Previous' }))
      expect(screen.getByRole('button', { name: 'Page 2' })).toHaveAttribute('aria-current', 'true')
      fireEvent.click(screen.getByRole('button', { name: 'Previous' }))
      expect(screen.getByRole('button', { name: 'Page 1' })).toHaveAttribute('aria-current', 'true')
    } finally {
      HTMLElement.prototype.scrollTo = original
    }
  })
})
