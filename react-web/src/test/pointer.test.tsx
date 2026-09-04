import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'
import { renderSession, enterAsGuest } from './harness'
import { Pointer } from '../features/pointer/Pointer'
import type { HoverAction, ProximityTip } from '../engine/protocol'

// DOMAIN: pointer — the world-entity hover/cursor-lock/proximity streams (scene → page only).
describe('pointer domain', () => {
  it('hover stream populates the reticle actions', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({
      kind: 'hover',
      actions: [
        { button: 1, text: 'Sit here', enabled: true },
        { button: 0, text: 'Too far', enabled: false }
      ]
    })
    expect(h.session().hover).toHaveLength(2)
    expect(h.session().hover[0]).toMatchObject({ button: 1, text: 'Sit here', enabled: true })
  })

  it('an empty hover stream clears the reticle prompts', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'hover', actions: [{ button: 1, text: 'Open', enabled: true }] })
    expect(h.session().hover).toHaveLength(1)
    h.driver.emit({ kind: 'hover', actions: [] })
    expect(h.session().hover).toHaveLength(0)
  })

  it('cursorLock stream toggles the crosshair flag', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().cursorLocked).toBe(false)
    h.driver.emit({ kind: 'cursorLock', locked: true })
    expect(h.session().cursorLocked).toBe(true)
    h.driver.emit({ kind: 'cursorLock', locked: false })
    expect(h.session().cursorLocked).toBe(false)
  })

  it('proximity stream carries the projected tooltips', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'proximity', tips: [{ id: 7, x: 100, y: 200, actions: [{ button: 0, text: 'Sit', enabled: true }] }] })
    expect(h.session().proximity).toHaveLength(1)
    expect(h.session().proximity[0]).toMatchObject({ id: 7, x: 100, y: 200 })
    h.driver.emit({ kind: 'proximity', tips: [] })
    expect(h.session().proximity).toHaveLength(0)
  })
})

// COMPONENT: the Pointer overlay actually renders the crosshair / chips from that state.
describe('Pointer overlay rendering', () => {
  it('renders the crosshair reticle when the cursor is locked (engine-relayed)', () => {
    const { getByTestId } = render(<Pointer locked={true} hover={[]} proximity={[]} />)
    expect(getByTestId('reticle')).toBeTruthy()
  })

  it('renders no reticle and nothing else when idle (unlocked, no hover, no proximity)', () => {
    const { container, queryByTestId } = render(<Pointer locked={false} hover={[]} proximity={[]} />)
    expect(queryByTestId('reticle')).toBeNull()
    expect(container.firstChild).toBeNull()
  })

  it('shows the camera-gated hint (default reason) but no reticle when unlocked', () => {
    const hover: HoverAction[] = [
      { button: 1, text: 'Open', enabled: true },
      { button: 0, text: 'Pick up', enabled: false }
    ]
    const { getByText, queryByTestId } = render(<Pointer locked={false} hover={hover} proximity={[]} />)
    expect(getByText('Open')).toBeTruthy()
    expect(getByText('Get camera closer')).toBeTruthy()
    expect(queryByTestId('reticle')).toBeNull()
  })

  it('shows the camera glyph + "Get camera closer" when the camera-distance rule gates it', () => {
    const hover: HoverAction[] = [{ button: 0, text: 'Show Profile', enabled: false, tooFarReason: 'camera' }]
    const { queryByTestId, getByText } = render(<Pointer locked={false} hover={hover} proximity={[]} />)
    expect(queryByTestId('too-far-icon-camera')).toBeTruthy()
    expect(getByText('Get camera closer')).toBeTruthy()
  })

  it('shows the walking glyph + "Get player closer" when the player-distance rule gates it', () => {
    const hover: HoverAction[] = [{ button: 0, text: 'Show Profile', enabled: false, tooFarReason: 'player' }]
    const { queryByTestId, getByText } = render(<Pointer locked={false} hover={hover} proximity={[]} />)
    expect(queryByTestId('too-far-icon-player')).toBeTruthy()
    expect(getByText('Get player closer')).toBeTruthy()
  })

  it('anchors each proximity tooltip at its projected screen coords', () => {
    const tips: ProximityTip[] = [{ id: 3, x: 120, y: 240, actions: [{ button: 0, text: 'Sit', enabled: true }] }]
    const { getByText, getByTestId } = render(<Pointer locked={false} hover={[]} proximity={tips} />)
    expect(getByText('Sit')).toBeTruthy()
    const tip = getByTestId('proxtip-3')
    expect(tip.style.left).toBe('120px')
    expect(tip.style.top).toBe('240px')
  })
})
