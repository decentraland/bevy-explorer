import { describe, it, expect } from 'vitest'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: pointer — the world-entity hover/reticle stream (scene → page only).
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
})
