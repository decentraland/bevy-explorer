import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import { SurfaceBoundary } from '../features/error/SurfaceBoundary'
import { PopupHost, resetPopups, openPopup, closeTopPopup, hasOpenPopup } from '../design/popups'

let broken = true
function Page(): React.JSX.Element {
  if (broken) throw new Error('page bug')
  return <p>backpack body</p>
}

function Hud({ open, onCrash }: { open: boolean; onCrash: () => void }): React.JSX.Element {
  return (
    <>
      <p>sidebar</p>
      <SurfaceBoundary name="Backpack" open={open} onCrash={onCrash}>
        {open && <Page />}
      </SurfaceBoundary>
      <PopupHost />
    </>
  )
}

afterEach(() => {
  act(() => resetPopups())
  vi.restoreAllMocks()
})

describe('SurfaceBoundary', () => {
  it('a crashing page closes and tells the user while the rest of the HUD keeps running', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    broken = true
    const onCrash = vi.fn()
    render(<Hud open onCrash={onCrash} />)
    expect(screen.getByText('sidebar')).toBeTruthy()
    expect(onCrash).toHaveBeenCalledTimes(1)
    expect(await screen.findByText('Backpack ran into a problem')).toBeTruthy()
  })

  it('reopening retries the page', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    broken = true
    const onCrash = vi.fn()
    const r = render(<Hud open onCrash={onCrash} />)
    r.rerender(<Hud open={false} onCrash={onCrash} />)
    broken = false
    r.rerender(<Hud open onCrash={onCrash} />)
    expect(screen.getByText('backpack body')).toBeTruthy()
  })
})

describe('popup crash', () => {
  it('closes only the popup that crashed', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    render(
      <>
        <p>sidebar</p>
        <PopupHost />
      </>
    )
    act(() => {
      openPopup(() => <p>passport</p>)
    })
    act(() => {
      openPopup(() => {
        throw new Error('popup bug')
      })
    })
    expect(screen.getByText('sidebar')).toBeTruthy()
    expect(screen.getByText('passport')).toBeTruthy()
    act(() => closeTopPopup())
    expect(hasOpenPopup()).toBe(false)
  })
})
