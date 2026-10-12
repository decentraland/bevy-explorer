import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { BackpackPage } from '../features/backpack/BackpackPage'
import { previewFocusFor } from '../features/backpack/previewFocus'
import { fakeProfileState, fakeSession } from './harness'

describe('avatar preview focus', () => {
  it('maps categories to the part of the avatar the camera frames', () => {
    expect(['eyes', 'hair', 'hat', 'mask', 'facial_hair'].map(previewFocusFor)).toEqual(['head', 'head', 'head', 'head', 'head'])
    expect(previewFocusFor('upper_body')).toBe('top')
    expect(previewFocusFor('lower_body')).toBe('bottom')
    expect(previewFocusFor('feet')).toBe('shoes')
    expect(['all', 'body_shape', 'hands_wear', 'skin'].map(previewFocusFor)).toEqual(['body', 'body', 'body', 'body'])
  })

  it('picking a category moves the camera, and clearing it goes back to the whole avatar', () => {
    const s = fakeSession()
    const backpack = { ...s.backpack, open: true, list: [{ urn: 'urn:x', name: 'X', rarity: 'base', category: 'eyes', equipped: false }], total: 1, focus: vi.fn() }
    render(<BackpackPage backpack={backpack} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
    expect(backpack.focus).toHaveBeenLastCalledWith('body')
    fireEvent.click(screen.getByRole('button', { name: 'Eyes' }))
    expect(backpack.focus).toHaveBeenLastCalledWith('head')
    fireEvent.click(screen.getByRole('button', { name: 'Feet' }))
    expect(backpack.focus).toHaveBeenLastCalledWith('shoes')
    fireEvent.click(screen.getByRole('tab', { name: /emotes/i }))
    expect(backpack.focus).toHaveBeenLastCalledWith('body')
  })
})
