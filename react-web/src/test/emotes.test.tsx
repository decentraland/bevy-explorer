import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { Emote } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: emotes — equipped 10-slot wheel, play (trigger) an emote.
describe('emotes domain', () => {
  const emote = (slot: number, urn: string): Emote => ({ slot, urn, name: urn })

  it('opening the emote wheel fetches emotes once', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().emotes.toggle())
    expect(h.session().emotes.open).toBe(true)
    expect(h.driver.sentOf('getEmotes')).toHaveLength(1)
  })

  it('emotes stream populates the wheel', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'emotes', emotes: [emote(0, 'urn:wave'), emote(1, 'urn:dance')] })
    expect(h.session().emotes.list).toHaveLength(2)
  })

  it('play triggers the emote and closes the wheel', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().emotes.toggle())
    act(() => h.session().emotes.play('urn:wave'))
    expect(h.driver.last('triggerEmote')).toEqual({ kind: 'triggerEmote', urn: 'urn:wave' })
    expect(h.session().emotes.open).toBe(false)
  })
})
