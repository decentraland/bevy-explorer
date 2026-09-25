import { describe, it, expect, vi, afterEach } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { SceneLoadingOverlay } from '../features/session/SceneLoadingOverlay'
import { LOADING_TIPS, TIP_ROTATE_MS } from '../features/session/loadingTips'

const loading = (over: Partial<{ realmConnected: boolean; pendingAssets: number | null }> = {}) => ({
  visible: true,
  realmConnected: true,
  title: '',
  pendingAssets: null,
  ...over
})

afterEach(() => vi.useRealTimers())

// Unity SceneLoadingScreenView: top bar with LOADING N%, a tips carousel rotating every 10s.
describe('loading screen', () => {
  it('shows a tip with its illustration and one dot per tip', () => {
    render(<SceneLoadingOverlay scene={loading()} />)
    expect(screen.getByRole('heading', { name: LOADING_TIPS[0].title })).toBeInTheDocument()
    expect(screen.getByRole('img', { name: LOADING_TIPS[0].title })).toHaveAttribute('src', LOADING_TIPS[0].image)
    expect(screen.getAllByRole('tab')).toHaveLength(LOADING_TIPS.length)
  })

  it('rotates every 10s and the arrows and dots move between tips', () => {
    vi.useFakeTimers()
    render(<SceneLoadingOverlay scene={loading()} />)
    act(() => vi.advanceTimersByTime(TIP_ROTATE_MS))
    expect(screen.getByRole('heading', { name: LOADING_TIPS[1].title })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Previous tip' }))
    expect(screen.getByRole('heading', { name: LOADING_TIPS[0].title })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Previous tip' }))
    expect(screen.getByRole('heading', { name: LOADING_TIPS[LOADING_TIPS.length - 1].title })).toBeInTheDocument()
    fireEvent.click(screen.getAllByRole('tab')[3])
    expect(screen.getByRole('heading', { name: LOADING_TIPS[3].title })).toBeInTheDocument()
  })

  it('names the emote key instead of a placeholder', () => {
    const i = LOADING_TIPS.findIndex((t) => t.body.includes('{Emote}'))
    render(<SceneLoadingOverlay scene={loading()} />)
    fireEvent.click(screen.getAllByRole('tab')[i])
    expect(screen.queryByText(/\{Emote\}/)).toBeNull()
    expect(screen.getByText('B')).toBeInTheDocument()
  })

  it('reports progress as LOADING N% once assets are counted, and RECONNECTING when the realm drops', () => {
    const { rerender } = render(<SceneLoadingOverlay scene={loading({ pendingAssets: 10 })} />)
    rerender(<SceneLoadingOverlay scene={loading({ pendingAssets: 6 })} />)
    expect(screen.getByText('LOADING 40%')).toBeInTheDocument()
    rerender(<SceneLoadingOverlay scene={loading({ realmConnected: false })} />)
    expect(screen.getByText('RECONNECTING…')).toBeInTheDocument()
  })
})
