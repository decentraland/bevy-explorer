import { describe, it, expect, vi } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { GalleryPage } from '../features/gallery/GalleryPage'
import type { GalleryState, ProfileState } from '../features/session/useEngineSession'
import type { GalleryPhoto } from '../engine/protocol'

const profile: ProfileState = {
  data: { address: '0xme', name: 'Mojito', hasClaimedName: true, isGuest: false },
  open: false,
  toggle: vi.fn()
}

function gallery(over: Partial<GalleryState> = {}): GalleryState {
  return {
    list: [],
    current: 0,
    max: 0,
    loaded: true,
    open: true,
    toggle: vi.fn(),
    metas: {},
    loadPhoto: vi.fn(),
    remove: vi.fn(),
    ...over
  }
}

// Two photos in different months (unix seconds) → exercises month grouping.
const FEB = '1707000000' // ~Feb 2024
const JAN = '1704600000' // ~Jan 2024
const photos: GalleryPhoto[] = [
  { id: 'p-feb', url: 'https://img/feb.jpg', thumbnailUrl: 'https://img/feb_t.jpg', dateTime: FEB },
  { id: 'p-jan', url: 'https://img/jan.jpg', thumbnailUrl: 'https://img/jan_t.jpg', dateTime: JAN }
]

function renderPage(g: GalleryState, props: Partial<Parameters<typeof GalleryPage>[0]> = {}) {
  return render(
    <GalleryPage
      gallery={g}
      profile={profile}
      onNavigate={vi.fn()}
      onTeleport={props.onTeleport ?? vi.fn()}
      onViewProfile={props.onViewProfile ?? vi.fn()}
    />
  )
}

describe('GalleryPage', () => {
  it('renders nothing when closed', () => {
    const { container } = renderPage(gallery({ open: false }))
    expect(container).toBeEmptyDOMElement()
  })

  it('shows the empty state when loaded with no photos', () => {
    renderPage(gallery({ loaded: true, list: [] }))
    expect(screen.getByText(/No photos yet/i)).toBeInTheDocument()
  })

  it('groups photos by month and shows storage usage', () => {
    renderPage(gallery({ list: photos, current: 2, max: 500 }))
    expect(screen.getByRole('heading', { name: 'February 2024' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'January 2024' })).toBeInTheDocument()
    expect(screen.getAllByRole('button', { name: 'Open photo' })).toHaveLength(2)
    expect(screen.getByText('2 / 500 photos')).toBeInTheDocument()
  })

  it('opening a photo shows the detail with the full-res image and requests its metadata', async () => {
    const g = gallery({ list: photos })
    renderPage(g)
    await userEvent.click(screen.getAllByRole('button', { name: 'Open photo' })[0])
    const dialog = screen.getByRole('dialog', { name: 'Photo' })
    // Download links the full-res url of the newest (February) photo.
    expect(within(dialog).getByRole('link', { name: /Download/i })).toHaveAttribute('href', 'https://img/feb.jpg')
    expect(g.loadPhoto).toHaveBeenCalledWith('p-feb')
  })

  it('Jump In teleports to the photo parcel from its metadata', async () => {
    const onTeleport = vi.fn()
    const g = gallery({ list: photos, metas: { 'p-feb': { sceneName: 'Genesis Plaza', x: -9, y: 14 } } })
    renderPage(g, { onTeleport })
    await userEvent.click(screen.getAllByRole('button', { name: 'Open photo' })[0])
    await userEvent.click(screen.getByRole('button', { name: /Jump In/i }))
    expect(onTeleport).toHaveBeenCalledWith(-9, 14)
  })

  it('delete asks to confirm, then removes the photo', async () => {
    const g = gallery({ list: photos, metas: { 'p-feb': {} } })
    renderPage(g)
    await userEvent.click(screen.getAllByRole('button', { name: 'Open photo' })[0])
    const del = screen.getByRole('button', { name: /Delete/i })
    await userEvent.click(del)
    expect(g.remove).not.toHaveBeenCalled()
    await userEvent.click(screen.getByRole('button', { name: /Confirm/i }))
    expect(g.remove).toHaveBeenCalledWith('p-feb')
  })
})
