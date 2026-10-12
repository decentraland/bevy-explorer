import { describe, it, expect, vi, afterEach } from 'vitest'
import { act, cleanup, render } from '@testing-library/react'
import { hudInsetRef, useHudInsetReport, type InteractableArea } from '../lib/hudInset'

// jsdom lays nothing out, so each element's on-screen rect is stubbed from its data attributes.
function Block({ right, height = 10 }: { right: number; height?: number }): React.JSX.Element {
  return (
    <div
      ref={(el) => {
        if (el == null) return
        el.getBoundingClientRect = () => ({ left: 0, top: 0, right, bottom: height, width: right, height, x: 0, y: 0, toJSON: () => ({}) })
        return hudInsetRef(el)
      }}
    />
  )
}

function Hud({ report, enabled = true, chat, map = 274 }: { report: (a: InteractableArea) => void; enabled?: boolean; chat?: number; map?: number | null }): React.JSX.Element {
  useHudInsetReport(report, enabled)
  return (
    <>
      {map != null && <Block right={map} />}
      {chat != null && <Block right={chat} />}
    </>
  )
}

const flush = (): Promise<void> => act(async () => { await Promise.resolve() })

afterEach(cleanup)

// DOMAIN: hudInset — the interactable-area inset is the rightmost edge of the mounted persistent
// HUD elements, re-reported as they mount/unmount, and frozen while reporting is disabled.
describe('hud inset', () => {
  it('reports the rightmost edge of the registered elements', async () => {
    const report = vi.fn()
    render(<Hud report={report} chat={398} />)
    await flush()
    expect(report).toHaveBeenCalledTimes(1)
    expect(report).toHaveBeenLastCalledWith({ left: 398, top: 0, right: 0, bottom: 0 })
  })

  it('shrinks to the remaining element when the wider one unmounts, and grows back', async () => {
    const report = vi.fn()
    const h = render(<Hud report={report} chat={398} />)
    await flush()
    h.rerender(<Hud report={report} />)
    await flush()
    expect(report).toHaveBeenLastCalledWith({ left: 274, top: 0, right: 0, bottom: 0 })
    h.rerender(<Hud report={report} chat={398} />)
    await flush()
    expect(report).toHaveBeenLastCalledWith({ left: 398, top: 0, right: 0, bottom: 0 })
    expect(report).toHaveBeenCalledTimes(3)
  })

  it('reports nothing when the layout is unchanged', async () => {
    const report = vi.fn()
    const h = render(<Hud report={report} chat={398} />)
    await flush()
    h.rerender(<Hud report={report} chat={398} />)
    await flush()
    expect(report).toHaveBeenCalledTimes(1)
  })

  it('keeps the last value while disabled, even as the HUD unmounts under a full-screen page', async () => {
    const report = vi.fn()
    const h = render(<Hud report={report} chat={398} />)
    await flush()
    h.rerender(<Hud report={report} enabled={false} map={null} />)
    await flush()
    expect(report).toHaveBeenCalledTimes(1)
    h.rerender(<Hud report={report} chat={398} />)
    await flush()
    expect(report).toHaveBeenLastCalledWith({ left: 398, top: 0, right: 0, bottom: 0 })
  })

  it('reports zero with no HUD mounted', async () => {
    const report = vi.fn()
    render(<Hud report={report} map={null} />)
    await flush()
    expect(report).toHaveBeenLastCalledWith({ left: 0, top: 0, right: 0, bottom: 0 })
  })
})
