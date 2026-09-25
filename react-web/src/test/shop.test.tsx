import { describe, it, expect, vi, afterEach } from 'vitest'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ShopPage } from '../features/shop/ShopPage'
import { formatMana, shopItemPrice, type ShopItem } from '../features/shop/shopApi'
import { fakeProfileState } from './harness'

const item = (over: Partial<ShopItem> = {}): ShopItem => ({
  id: '0xabc-1',
  name: 'Neon Tiara',
  thumbnail: 'https://example.com/t.png',
  rarity: 'epic',
  category: 'wearable',
  url: '/contracts/0xabc/items/1',
  isOnSale: true,
  price: '2500000000000000000',
  available: 10,
  ...over
})

function serve(...responses: (ShopItem[] | 'down')[]): ReturnType<typeof vi.fn> {
  const f = vi.fn()
  for (const r of responses) f.mockResolvedValueOnce(r === 'down' ? new Response('', { status: 503 }) : new Response(JSON.stringify({ data: r, total: r.length })))
  vi.stubGlobal('fetch', f)
  return f
}

afterEach(() => vi.unstubAllGlobals())

// Unity ExploreSections.Shop — the in-client shop; purchases complete on the web marketplace.
describe('shop', () => {
  it('prices items in MANA: primary sale price, else the cheapest listing, else not for sale', () => {
    expect(formatMana('2500000000000000000')).toBe('2.5')
    expect(formatMana('0')).toBe('0')
    expect(shopItemPrice(item())).toBe('2.5 MANA')
    expect(shopItemPrice(item({ isOnSale: false, minListingPrice: '1000000000000000000' }))).toBe('1 MANA')
    expect(shopItemPrice(item({ isOnSale: false, minListingPrice: null }))).toBe('Not for sale')
    expect(shopItemPrice(item({ price: '0' }))).toBe('Free')
  })

  it('lists wearables with their price and a Buy link to the marketplace item', async () => {
    const f = serve([item()])
    render(<ShopPage shop={{ open: true, toggle: vi.fn() }} profile={fakeProfileState()} onNavigate={vi.fn()} />)
    expect(String(f.mock.calls[0][0])).toContain('category=wearable')
    const card = (await screen.findByText('Neon Tiara')).closest('[data-rarity]') as HTMLElement
    expect(within(card).getByText('2.5 MANA')).toBeInTheDocument()
    expect(within(card).getByRole('link', { name: 'Buy' })).toHaveAttribute('href', 'https://decentraland.org/marketplace/contracts/0xabc/items/1?utm_source=client')
  })

  it('switches to emotes and searches', async () => {
    const f = serve([item()], [item({ name: 'Wave' })], [item({ name: 'Wave' })])
    render(<ShopPage shop={{ open: true, toggle: vi.fn() }} profile={fakeProfileState()} onNavigate={vi.fn()} />)
    await screen.findByText('Neon Tiara')
    await userEvent.click(screen.getByRole('tab', { name: 'Emotes' }))
    await waitFor(() => expect(String(f.mock.calls[1][0])).toContain('category=emote'))
    fireEvent.change(screen.getByPlaceholderText('Search the shop'), { target: { value: 'wave' } })
    await waitFor(() => expect(String(f.mock.calls[2][0])).toContain('search=wave'))
  })

  it('a new query starts blank; a revisited one shows its last results dimmed until the refetch answers', async () => {
    const answers: ((r: Response) => void)[] = []
    const f = serve([item()])
    for (let i = 0; i < 2; i++) f.mockReturnValueOnce(new Promise<Response>((resolve) => answers.push(resolve)))
    const reply = (items: ShopItem[]): void => answers.shift()!(new Response(JSON.stringify({ data: items, total: items.length })))
    const busy = (name: string): string | null => screen.getByText(name).closest('[aria-busy]')!.getAttribute('aria-busy')
    render(<ShopPage shop={{ open: true, toggle: vi.fn() }} profile={fakeProfileState()} onNavigate={vi.fn()} />)
    await screen.findByText('Neon Tiara')
    expect(busy('Neon Tiara')).toBe('false')

    await userEvent.click(screen.getByRole('tab', { name: 'Emotes' }))
    expect(screen.queryByText('Neon Tiara')).toBeNull()
    reply([item({ id: 'w', name: 'Wave' })])
    expect(await screen.findByText('Wave')).toBeInTheDocument()

    await userEvent.click(screen.getByRole('tab', { name: 'Wearables' }))
    expect(busy('Neon Tiara')).toBe('true')
    expect(screen.queryByText('Wave')).toBeNull()
    reply([item({ id: 'j', name: 'Pixel Jacket' })])
    expect(await screen.findByText('Pixel Jacket')).toBeInTheDocument()
    expect(busy('Pixel Jacket')).toBe('false')
  })

  it('shows the failure with Retry', async () => {
    serve('down', [item()])
    render(<ShopPage shop={{ open: true, toggle: vi.fn() }} profile={fakeProfileState()} onNavigate={vi.fn()} />)
    expect(await screen.findByText("Couldn't load the shop")).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }))
    expect(await screen.findByText('Neon Tiara')).toBeInTheDocument()
  })
})
