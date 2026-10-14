import { describe, it, expect, vi, afterEach } from 'vitest'
import { fireEvent, render, screen, within } from '@testing-library/react'
import { PlacesBrowser } from '../features/places/PlacesBrowser'
import type { DiscoverPlace } from '../features/places/placesApi'

const place = (id: string, title: string, extra: Partial<DiscoverPlace> = {}): DiscoverPlace => ({
  id,
  title,
  description: '',
  image: `https://img/${id}.png`,
  positions: ['1,1'],
  base_position: '1,1',
  owner: '0xabc',
  ...extra
})

const busy = place('busy', 'Busy Club', { user_count: 5 })
const empty = place('empty', 'Quiet Gallery', { user_count: 0 })
const curated = place('curated', 'Curated World', { world: true, world_name: 'curated.dcl.eth', highlighted: true })
const other = place('other', 'Other Scene')
const junk = place('junk', 'Empty', { image: 'https://api.decentraland.org/v2/map.png' })

function servePlaces(): void {
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string) => {
      const data = url.includes('only_highlighted=true') ? [curated] : url.includes('limit=40') ? [busy, empty] : [busy, empty, curated, other, junk]
      return Promise.resolve(new Response(JSON.stringify({ ok: true, total: data.length, data })))
    })
  )
}

afterEach(() => vi.unstubAllGlobals())

describe('places browser', () => {
  it('keeps Live Now to populated destinations, Featured to the curated set, and the grid to the rest', async () => {
    servePlaces()
    render(<PlacesBrowser onPick={vi.fn()} />)
    const live = (await screen.findByRole('heading', { name: /Live Now/ })).closest('section') as HTMLElement
    const featured = (await screen.findByRole('heading', { name: 'Featured Places' })).closest('section') as HTMLElement

    expect(within(live).getByRole('button', { name: /Busy Club/ })).toBeInTheDocument()
    expect(within(live).queryByRole('button', { name: /Quiet Gallery/ })).not.toBeInTheDocument()
    expect(within(featured).getByRole('button', { name: /Curated World/ })).toBeInTheDocument()

    expect(screen.getAllByRole('button', { name: /Busy Club/ })).toHaveLength(1)
    expect(screen.getByRole('button', { name: /Quiet Gallery/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Other Scene/ })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /\bEmpty\b/ })).not.toBeInTheDocument()
  })

  it('shows a search match even when its record looks like an unfinished deploy', async () => {
    const untitled = place('tortilla', 'My Scene', {
      world: true,
      world_name: 'tortilla.dcl.eth',
      image: 'https://peer/content/contents/bafkreidj26s7aenyxfthfdibnqonzqm5ptc4iamml744gmcyuokewkr76y',
      user_count: 0
    })
    vi.stubGlobal(
      'fetch',
      vi.fn((url: string) => {
        const data = url.includes('search=tortilla') ? [untitled] : []
        return Promise.resolve(new Response(JSON.stringify({ ok: true, total: data.length, data })))
      })
    )
    render(<PlacesBrowser onPick={vi.fn()} />)
    fireEvent.change(screen.getByPlaceholderText('Search places'), { target: { value: 'tortilla' } })
    expect(await screen.findByRole('button', { name: /My Scene/ }, { timeout: 2000 })).toBeInTheDocument()
  })
})
