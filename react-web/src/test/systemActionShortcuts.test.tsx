// Engine-driven menu hotkeys: systemAction stream messages toggle the panels (replacing the
// old DOM keydown table in useMenuShortcuts), with the same guards applied at event receipt —
// plus the bindings slice (table state, whole-table set, press-a-key capture id round-trip).
import { describe, it, expect, afterEach, vi } from 'vitest'
import { act, render, screen, waitFor } from '@testing-library/react'
import { renderSession, enterAsGuest, type Harness } from './harness'
import { openPopup, resetPopups, PopupHost } from '../design'
import { setBindingsSnapshot } from '../lib/bindingLabels'
import { registerCancelLayer } from '../lib/cancelLayers'
import { lockInput } from '../lib/inputLock'

async function world(): Promise<Harness> {
  const h = renderSession({ userId: null })
  await enterAsGuest(h)
  return h
}

const action = (name: string, pressed = true): { kind: 'systemAction'; action: string; pressed: boolean } => ({
  kind: 'systemAction',
  action: name,
  pressed
})

afterEach(() => {
  resetPopups()
  act(() => setBindingsSnapshot([])) // module store leaks across tests
  document.body.innerHTML = ''
})

describe('system-action menu shortcuts', () => {
  it('toggles the matching panel on a pressed edge', async () => {
    const h = await world()
    h.driver.emit(action('Places'))
    expect(h.session().places.open).toBe(true)
    h.driver.emit(action('Places'))
    expect(h.session().places.open).toBe(false)
    h.driver.emit(action('Map'))
    expect(h.session().map.open).toBe(true)
    // exclusive: opening another panel closes the previous one
    h.driver.emit(action('Gallery'))
    expect(h.session().map.open).toBe(false)
    expect(h.session().gallery.open).toBe(true)
    h.driver.emit(action('ChatPanel'))
    expect(h.session().gallery.open).toBe(false)
    expect(h.session().chat.open).toBe(true)
  })

  it('ignores release edges', async () => {
    const h = await world()
    h.driver.emit(action('Places', false))
    expect(h.session().places.open).toBe(false)
  })

  it('does nothing outside the world phase', () => {
    const h = renderSession({ userId: null })
    h.driver.emit(action('Places'))
    expect(h.session().places.open).toBe(false)
  })

  it('is inert while a popup is open (modal owns the keyboard)', async () => {
    const h = await world()
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>a popup</div>)
    })
    expect(screen.getByText('a popup')).toBeTruthy()
    h.driver.emit(action('Places'))
    expect(h.session().places.open).toBe(false)
  })

  it('is inert while a text input holds focus (the engine cannot see DOM focus)', async () => {
    const h = await world()
    const input = document.createElement('input')
    document.body.appendChild(input)
    input.focus()
    h.driver.emit(action('Places'))
    expect(h.session().places.open).toBe(false)
  })

  it("a 'Cancel' action (the gamepad route) closes the topmost popup, then open panels", async () => {
    const h = await world()
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>a popup</div>)
    })
    act(() => h.driver.emit(action('Cancel')))
    expect(screen.queryByText('a popup')).toBeNull()
    // No popup left → the same action closes whatever panel is open.
    h.driver.emit(action('Places'))
    expect(h.session().places.open).toBe(true)
    h.driver.emit(action('Cancel'))
    expect(h.session().places.open).toBe(false)
  })

  it("a 'Cancel' action peels one registered leaf layer (lightbox/dropdown) before panels", async () => {
    const h = await world()
    h.driver.emit(action('Gallery'))
    expect(h.session().gallery.open).toBe(true)
    const closed = vi.fn()
    const unregister = registerCancelLayer(() => {
      closed()
      unregister() // the layer closes → it leaves the stack, like a real widget unmounting
    })
    h.driver.emit(action('Cancel'))
    expect(closed).toHaveBeenCalledTimes(1)
    expect(h.session().gallery.open).toBe(true) // one layer per press — the panel survives
    h.driver.emit(action('Cancel'))
    expect(h.session().gallery.open).toBe(false) // next press falls through to the panel
  })

  it("a popup wins over a registered leaf layer (it renders on top of everything)", async () => {
    const h = await world()
    render(<PopupHost />)
    const closed = vi.fn()
    const unregister = registerCancelLayer(closed)
    act(() => {
      openPopup(() => <div>a popup</div>)
    })
    act(() => h.driver.emit(action('Cancel')))
    expect(screen.queryByText('a popup')).toBeNull()
    expect(closed).not.toHaveBeenCalled() // the layer waits its turn
    unregister()
  })

  it("a 'Cancel' action is ignored while the input lock is held (crash modal)", async () => {
    const h = await world()
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>a popup</div>)
    })
    const unlock = lockInput()
    act(() => h.driver.emit(action('Cancel')))
    expect(screen.getByText('a popup')).toBeTruthy() // frozen — nothing closes invisibly
    unlock()
    act(() => h.driver.emit(action('Cancel')))
    expect(screen.queryByText('a popup')).toBeNull()
  })

  it('pre-world, the cancel key closes popups straight from the DOM (no stream exists yet)', () => {
    renderSession({ userId: null }) // login phase — installs the fallback handler
    render(<PopupHost />)
    act(() => {
      openPopup(() => <div>realm error</div>)
    })
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    })
    expect(screen.queryByText('realm error')).toBeNull()
  })

  it("declares uiFocus to the engine: panels/popups set ui, a focused text field sets text", async () => {
    const h = await world()
    h.driver.clearSent()
    h.driver.emit(action('Places'))
    await waitFor(() =>
      expect(h.driver.last('uiFocus')).toEqual({ kind: 'uiFocus', ui: true, text: false, scroll: false })
    )
    h.driver.emit(action('Places'))
    await waitFor(() =>
      expect(h.driver.last('uiFocus')).toEqual({ kind: 'uiFocus', ui: false, text: false, scroll: false })
    )

    const input = document.createElement('input')
    document.body.appendChild(input)
    act(() => input.focus())
    await waitFor(() =>
      expect(h.driver.last('uiFocus')).toEqual({ kind: 'uiFocus', ui: false, text: true, scroll: false })
    )
  })

  it('Scroll edges scroll the hovered scrollable panel while held (gamepad/key scrolling)', async () => {
    const h = await world()
    const panel = document.createElement('div')
    document.body.appendChild(panel)
    Object.defineProperty(panel, 'scrollHeight', { value: 200 })
    Object.defineProperty(panel, 'clientHeight', { value: 100 })
    const scrollBy = vi.fn()
    ;(panel as unknown as { scrollBy: typeof scrollBy }).scrollBy = scrollBy
    const style = vi
      .spyOn(window, 'getComputedStyle')
      .mockReturnValue({ overflowY: 'auto', overflowX: 'hidden' } as unknown as CSSStyleDeclaration)
    act(() => {
      panel.dispatchEvent(new Event('pointerover', { bubbles: true })) // hover the panel
    })
    h.driver.emit(action('ScrollDown'))
    await waitFor(() =>
      expect(scrollBy.mock.calls.some((c) => (c[0] as { top: number }).top > 0)).toBe(true)
    )
    h.driver.emit(action('ScrollDown', false)) // release stops the loop
    style.mockRestore()
  })

  it('quick emotes: QuickEmoteN plays that slot, but only while the wheel is open', async () => {
    const h = await world()
    h.driver.emit({ kind: 'emotes', emotes: [{ slot: 3, urn: 'urn:emote:three', name: 'Three' }] })

    // Wheel closed → nothing plays.
    h.driver.emit(action('QuickEmote3'))
    expect(h.driver.last('triggerEmote')).toBeUndefined()

    // Wheel open → slot 3 plays (and the wheel closes); an empty slot (7) is a no-op.
    act(() => h.session().emotes.toggle())
    h.driver.emit(action('QuickEmote3'))
    expect(h.driver.last('triggerEmote')).toEqual({ kind: 'triggerEmote', urn: 'urn:emote:three' })
    h.driver.clearSent()
    act(() => h.session().emotes.toggle())
    h.driver.emit(action('QuickEmote7'))
    expect(h.driver.last('triggerEmote')).toBeUndefined()
  })
})

describe('bindings slice', () => {
  it('requests the table on world entry and mirrors the bindings message', async () => {
    const h = renderSession({ userId: null })
    await enterAsGuest(h, { keepSent: true }) // keep the world-entry auto-fetches visible
    expect(h.driver.sentOf('getBindings').length).toBeGreaterThan(0)
    h.driver.emit({ kind: 'bindings', bindings: [[{ System: 'Places' }, ['KeyZ']]] })
    expect(h.session().bindings.list).toEqual([[{ System: 'Places' }, ['KeyZ']]])
  })

  it('set posts the whole table; reset posts resetBindings', async () => {
    const h = await world()
    act(() => h.session().bindings.set([[{ System: 'Places' }, ['KeyX']]]))
    expect(h.driver.last('setBindings')).toEqual({
      kind: 'setBindings',
      bindings: [[{ System: 'Places' }, ['KeyX']]]
    })
    act(() => h.session().bindings.reset())
    expect(h.driver.last('resetBindings')).toBeTruthy()
  })

  it('capture resolves on the matching id and drops stale/cancelled ones', async () => {
    const h = await world()
    const first = h.session().bindings.capture()
    const firstId = h.driver.last('captureInput')?.id
    // Superseded by a second capture: the first must never resolve.
    const second = h.session().bindings.capture()
    const secondId = h.driver.last('captureInput')?.id
    expect(firstId).not.toBe(secondId)
    let firstResolved: string | null = null
    void first.input.then((v) => (firstResolved = v))
    h.driver.emit({ kind: 'inputCaptured', id: firstId ?? '', input: 'KeyQ' }) // stale → dropped
    h.driver.emit({ kind: 'inputCaptured', id: secondId ?? '', input: 'KeyX' })
    await expect(second.input).resolves.toBe('KeyX')
    expect(firstResolved).toBeNull()

    // A cancelled capture drops the engine's late resolution.
    const third = h.session().bindings.capture()
    const thirdId = h.driver.last('captureInput')?.id
    let thirdResolved: string | null = null
    void third.input.then((v) => (thirdResolved = v))
    third.cancel()
    h.driver.emit({ kind: 'inputCaptured', id: thirdId ?? '', input: 'KeyY' })
    await waitFor(() => expect(thirdResolved).toBeNull())
  })
})
