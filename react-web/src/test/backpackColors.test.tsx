import { describe, it, expect, vi } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { BackpackPage } from '../features/backpack/BackpackPage'
import { hexToColor3 } from '../lib/color'
import { enterAsGuest, fakeProfileState, fakeSession, renderSession } from './harness'

// Audit backpack-emotes-5: no skin/hair/eye color editing (Unity WearablesColorPickerController).
describe('backpack colors', () => {
  it('shows the hair picker for hair-coloured categories and not for others', () => {
    const s = fakeSession()
    const backpack = { ...s.backpack, open: true, colors: { skin: '#ddb18f', hair: '#5b310f', eyes: '#20b3f6' }, setColor: vi.fn() }
    render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
    expect(screen.queryByRole('group', { name: 'Hair color' })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Facial Hair' }))
    expect(screen.getByRole('radio', { name: '#5b310f' })).toBeChecked()
    fireEvent.click(screen.getByRole('radio', { name: '#ffbe28' }))
    expect(backpack.setColor).toHaveBeenCalledWith('hair', '#ffbe28')
  })

  it('offers skin colour on the body shape category and eye colour on eyes', () => {
    const s = fakeSession()
    const backpack = { ...s.backpack, open: true, colors: { skin: '#ddb18f', hair: '#5b310f', eyes: '#20b3f6' }, setColor: vi.fn() }
    render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: 'Body Shape' }))
    expect(screen.getByRole('group', { name: 'Skin color' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Eyes' }))
    expect(screen.getByRole('group', { name: 'Eye color' })).toBeInTheDocument()
  })

  it('edits the look at once and deploys it when the backpack closes', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().backpack.toggle())
    act(() => h.driver.emit({ kind: 'wearables', equipped: [], colors: { skin: hexToColor3('#ddb18f'), hair: hexToColor3('#5b310f'), eyes: hexToColor3('#20b3f6') } }))
    expect(h.session().backpack.colors?.hair).toBe('#5b310f')
    act(() => h.session().backpack.setColor('hair', '#ffbe28'))
    expect(h.session().backpack.colors?.hair).toBe('#ffbe28')
    expect(h.driver.sentOf('setAvatarColor')).toEqual([{ kind: 'setAvatarColor', target: 'hair', color: hexToColor3('#ffbe28') }])
    expect(h.driver.sentOf('commitAvatar')).toHaveLength(0)
    act(() => h.session().backpack.toggle())
    expect(h.driver.sentOf('commitAvatar')).toHaveLength(1)
  })
})
