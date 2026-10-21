import { describe, it, expect, vi, beforeEach } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { SettingsPanel } from '../features/settings/SettingsPanel'
import type { Setting } from '../engine/protocol'
import type { SettingsState } from '../features/session/useEngineSession'
import { fakeProfileState, fakeSession } from './harness'

const base = (over: Partial<Setting>): Setting => ({
  name: 'x',
  category: 'general',
  description: '',
  minValue: 0,
  maxValue: 100,
  namedVariants: [],
  value: 0,
  default: 0,
  stepSize: 1,
  ...over
})

const TOGGLE = base({ name: 'fullscreen', category: 'general', namedVariants: [{ name: 'Off', description: '' }, { name: 'On', description: '' }], value: 0, default: 1, maxValue: 1 })
const SLIDER = base({ name: 'fov', category: 'general', value: 70, default: 60, minValue: 60, maxValue: 100, stepSize: 5 })
const SELECT = base({ name: 'quality', category: 'graphics', description: 'Overall quality.\n\nLow: fastest.', namedVariants: [{ name: 'Low', description: 'Overall quality.\n\nLow: fastest.' }, { name: 'Med', description: 'Overall quality.\n\nMed: balanced.' }, { name: 'High', description: 'Overall quality.\n\nHigh: prettiest.' }], value: 0, default: 1 })
// The engine's own panel has no label, so its descriptions often lead with the name.
const DESCRIBED = base({ name: 'Camera Sensitivity', category: 'controls', description: 'Camera Sensitivity\n\nControls the sensitivity of camera inputs.\n\nNOTE: not pointer inputs.' })
// f32 values widened to f64 by the bridge
const FLOAT = base({ name: 'Jump Height', category: 'controls', value: 19.950000762939453, minValue: 0.05000000074505806, maxValue: 20, stepSize: 0.05000000074505806 })

function renderPanel(): SettingsState {
  const settings: SettingsState = { ...fakeSession().settings, open: true, list: [TOGGLE, SLIDER, SELECT, DESCRIBED, FLOAT], set: vi.fn() }
  render(
    <SettingsPanel
      settings={settings}
      bindings={fakeSession().bindings}
      profile={fakeProfileState()}
      onNavigate={vi.fn()}
    />
  )
  return settings
}

describe('settings panel controls', () => {
  let settings: SettingsState
  beforeEach(() => {
    settings = renderPanel()
  })

  it('toggle control sets 1 when turned on', async () => {
    await userEvent.click(screen.getByRole('switch', { name: 'fullscreen' }))
    expect(vi.mocked(settings.set)).toHaveBeenCalledWith('fullscreen', 1)
  })

  it('slider arrows step the value by stepSize', async () => {
    await userEvent.click(screen.getByRole('button', { name: 'increase' }))
    expect(vi.mocked(settings.set)).toHaveBeenCalledWith('fov', 75)
    await userEvent.click(screen.getByRole('button', { name: 'decrease' }))
    expect(vi.mocked(settings.set)).toHaveBeenCalledWith('fov', 65)
  })

  it('reset all defaults sets every field in the active category to its default', async () => {
    await userEvent.click(screen.getByRole('button', { name: /Reset all defaults/i }))
    expect(vi.mocked(settings.set)).toHaveBeenCalledWith('fullscreen', 1)
    expect(vi.mocked(settings.set)).toHaveBeenCalledWith('fov', 60)
  })

  it('a description is the label tooltip, minus the leading name line; none for an empty one', async () => {
    await userEvent.click(screen.getByRole('tab', { name: 'Controls' }))
    const tips = screen.getAllByRole('tooltip').map((t) => t.textContent)
    expect(tips).toEqual(['Controls the sensitivity of camera inputs.\n\nNOTE: not pointer inputs.'])
  })

  it('shows a slider value at f32 precision, not the widened f64', async () => {
    await userEvent.click(screen.getByRole('tab', { name: 'Controls' }))
    expect(screen.getByText('19.95')).toBeTruthy()
    expect(screen.queryByText('19.950000762939453')).toBeNull()
  })

  it('switching category tab + selecting an option sets the variant index', async () => {
    await userEvent.click(screen.getByRole('tab', { name: 'Graphics' }))
    await userEvent.click(screen.getByRole('button', { name: 'quality' }))
    await userEvent.click(screen.getByRole('option', { name: 'High' }))
    expect(vi.mocked(settings.set)).toHaveBeenCalledWith('quality', 2)
  })

  it("an enum's tooltip is the selected variant's description, from the list already held", async () => {
    await userEvent.click(screen.getByRole('tab', { name: 'Graphics' }))
    expect(screen.getByRole('tooltip').textContent).toBe('Overall quality.\n\nLow: fastest.')
    cleanup()
    render(
      <SettingsPanel
        settings={{ ...settings, list: [{ ...SELECT, value: 2 }] }}
        bindings={fakeSession().bindings}
        profile={fakeProfileState()}
        onNavigate={vi.fn()}
      />
    )
    expect(screen.getByRole('tooltip').textContent).toBe('Overall quality.\n\nHigh: prettiest.')
  })
})

// An engine value below zero means the setting is not known yet (server-owned, e.g. DM
// privacy before the social service answers): rendered, but not editable.
describe('settings panel unset controls', () => {
  it('a select with a negative value is disabled and does not set', async () => {
    const UNSET = base({ name: 'dm_privacy', category: 'chat', namedVariants: [{ name: 'Everyone', description: '' }, { name: 'Only friends', description: '' }], value: -1, default: 0, maxValue: 2 })
    const settings: SettingsState = { ...fakeSession().settings, open: true, list: [UNSET], set: vi.fn() }
    render(<SettingsPanel settings={settings} bindings={fakeSession().bindings} profile={fakeProfileState()} onNavigate={vi.fn()} />)
    // two named choices that are not off/on show their names, not a switch
    expect(screen.queryByRole('switch', { name: 'dm_privacy' })).toBeNull()
    const select = screen.getByRole('button', { name: 'dm_privacy' })
    expect(select).toBeDisabled()
    await userEvent.click(select)
    expect(vi.mocked(settings.set)).not.toHaveBeenCalled()
  })
})
