import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { Setting } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: settings — fetch ExplorerSettings on open, change a setting.
describe('settings domain', () => {
  const setting: Setting = {
    name: 'master_volume',
    category: 'audio',
    description: 'Master volume',
    minValue: 0,
    maxValue: 100,
    namedVariants: [],
    value: 50,
    default: 80,
    stepSize: 1
  }

  it('fetches settings the first time the panel opens (cached after)', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().settings.toggle())
    expect(h.driver.sentOf('getSettings')).toHaveLength(1)
    act(() => h.session().settings.toggle()) // close
    act(() => h.session().settings.toggle()) // reopen — cached, no re-fetch
    expect(h.driver.sentOf('getSettings')).toHaveLength(1)
  })

  it('settings stream populates the list', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'settings', settings: [setting] })
    expect(h.session().settings.list[0].name).toBe('master_volume')
  })

  it('set posts setSetting with name + value', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().settings.set('master_volume', 75))
    expect(h.driver.last('setSetting')).toEqual({ kind: 'setSetting', name: 'master_volume', value: 75 })
  })
})
