import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { BackpackPage } from '../features/backpack/BackpackPage'
import type { Outfit } from '../engine/protocol'
import { fakeProfileState, fakeSession } from './harness'

const grey = { r: 0.5, g: 0.5, b: 0.5 }
const outfit: Outfit = { bodyShape: '', eyes: { color: grey }, hair: { color: grey }, skin: { color: grey }, wearables: ['urn:hat'], forceRender: [] }
const PICTURE = 'data:image/png;base64,AAAA'

describe('saved outfit pictures', () => {
  it('a saved outfit shows its picture, or a silhouette until it has one', async () => {
    const s = fakeSession()
    const outfits = [{ slot: 0, outfit, thumbnail: PICTURE }, { slot: 1, outfit }]
    render(<BackpackPage backpack={{ ...s.backpack, open: true, outfits }} emotes={s.emotes} profile={fakeProfileState()} onNavigate={vi.fn()} setEngineViewport={vi.fn()} />)
    await userEvent.click(screen.getByRole('tab', { name: /saved outfits/i }))
    expect(screen.getByRole('button', { name: 'Outfit 1' }).querySelector('img')).toHaveAttribute('src', PICTURE)
    expect(screen.getByRole('button', { name: 'Outfit 2' }).querySelector('img')).toBeNull()
  })
})
