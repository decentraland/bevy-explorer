import { describe, it, expect } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import { SceneLoadingOverlay } from '../features/session/SceneLoadingOverlay'
import { createLoadingProgress, type LoadingInput } from '../features/session/loadingProgress'
import { enterAsGuest, renderSession } from './harness'

const scene = (pendingAssets: number | null, visible = true, realmConnected = true) => ({ visible, realmConnected, title: '', pendingAssets })
const input = (over: Partial<LoadingInput>): LoadingInput => ({ scene: scene(null), playerReady: false, revealing: false, travelling: false, ...over })

// Reported: the bar ran 0→99%, dropped back to 0%, finished, then showed LOADING with no number
// while the loader kept going (it also waits for the player spawn and the render settle).
describe('loading progress', () => {
  it('never goes backwards when the scene starts a bigger batch of models', () => {
    const p = createLoadingProgress()
    const seen = [20, 10, 1, 40, 25, 0].map((n) => p.next(input({ scene: scene(n) })))
    for (let i = 1; i < seen.length; i++) expect(seen[i]).toBeGreaterThanOrEqual(seen[i - 1])
    expect(seen[2]).toBeGreaterThan(seen[0])
  })

  it('keeps counting up through the stages after the models finish', () => {
    const p = createLoadingProgress()
    const steps = [
      input({ scene: scene(10) }),
      input({ scene: scene(0) }),
      input({ scene: scene(null) }),
      input({ scene: scene(null, false) }),
      input({ scene: scene(null, false), playerReady: true }),
      input({ scene: scene(null, false), playerReady: true, revealing: true })
    ].map((s) => p.next(s))
    for (let i = 1; i < steps.length; i++) expect(steps[i]).toBeGreaterThan(steps[i - 1])
    expect(steps.at(-1)).toBeLessThan(100)
  })

  it('starts over for the next loader', () => {
    const p = createLoadingProgress()
    p.next(input({ scene: scene(null, false), playerReady: true }))
    p.reset()
    expect(p.next(input({ scene: scene(null, true, false) }))).toBeLessThan(10)
  })

  it('the session reports one progress that only rises while the loader is up', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    const values: number[] = []
    for (const n of [30, 5, 60, 0]) {
      act(() => h.driver.emit({ kind: 'sceneLoading', state: scene(n) }))
      values.push(h.session().loadingProgress)
    }
    act(() => h.driver.emit({ kind: 'sceneLoading', state: scene(null) }))
    values.push(h.session().loadingProgress)
    for (let i = 1; i < values.length; i++) expect(values[i]).toBeGreaterThanOrEqual(values[i - 1])
    expect(values.at(-1)).toBeGreaterThanOrEqual(80)
  })

  it('the overlay always shows a number', () => {
    const { rerender } = render(<SceneLoadingOverlay scene={scene(null)} progress={63} />)
    expect(screen.getByRole('status')).toHaveTextContent('LOADING 63%')
    rerender(<SceneLoadingOverlay scene={scene(null, true, false)} progress={63} />)
    expect(screen.getByRole('status')).toHaveTextContent('RECONNECTING…')
  })
})
