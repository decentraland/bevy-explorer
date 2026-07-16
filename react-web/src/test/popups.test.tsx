import { describe, it, expect, afterEach } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { PopupHost, openPopup, closeTopPopup, resetPopups } from '../design'

afterEach(resetPopups)

describe('popup stack', () => {
  it('closeTopPopup closes only the topmost popup (Escape is relayed here — see useEngineSession)', () => {
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>first</div>)
      openPopup(() => <div>second</div>)
    })
    expect(screen.getByText('first')).toBeTruthy()
    expect(screen.getByText('second')).toBeTruthy()

    act(() => closeTopPopup())
    expect(screen.queryByText('second')).toBeNull() // top closed
    expect(screen.getByText('first')).toBeTruthy() // the one below stays

    act(() => closeTopPopup())
    expect(screen.queryByText('first')).toBeNull()
  })

  it('closeTopPopup is a no-op when the stack is empty', () => {
    render(<PopupHost />)
    act(() => closeTopPopup()) // no throw
    expect(screen.queryByText('x')).toBeNull()
  })

  it('a backdrop popup closes on outside (backdrop) click; backdropClickCloses:false does not', () => {
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>plain</div>) // default { backdrop: true, backdropClickCloses: true }
    })
    const backdrop = document.querySelector('[class*="backdrop"]') as HTMLElement
    fireEvent.click(backdrop)
    expect(screen.queryByText('plain')).toBeNull()

    act(() => {
      openPopup(() => <div>locked</div>, { backdropClickCloses: false })
    })
    fireEvent.click(document.querySelector('[class*="backdrop"]') as HTMLElement)
    expect(screen.getByText('locked')).toBeTruthy() // stayed open
  })
})
