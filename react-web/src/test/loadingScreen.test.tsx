import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { SceneLoadingOverlay } from '../features/session/SceneLoadingOverlay'
import { GENERIC_TIPS, TIP_ROTATE_MS } from '../features/session/loadingTips'
import { countLaunch } from '../lib/launchCount'
import { resetFeatureFlags } from '../lib/featureFlags'

const loading = (over: Partial<{ realmConnected: boolean; pendingAssets: number | null }> = {}) => ({
  visible: true,
  realmConnected: true,
  title: '',
  pendingAssets: null,
  ...over
})

afterEach(() => {
  vi.useRealTimers()
  resetFeatureFlags()
})

const heading = (): string | null => screen.getByRole('heading').textContent

function flags(payload?: object) {
  return {
    flags: { 'explorer-alfa-audience-loading-screen-tips': payload != null },
    variants: payload != null ? { 'explorer-alfa-audience-loading-screen-tips': { name: 'tips', payload: { type: 'json', value: JSON.stringify(payload) } } } : {}
  }
}

describe('loading screen', () => {
  beforeEach(() => {
    localStorage.clear()
    countLaunch()
  })

  it('shows new players the new-player tips, with art and an action pill', () => {
    render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(heading()).toBe('Say Hi!')
    expect(screen.getByRole('img', { name: 'Say Hi!' })).toBeInTheDocument()
    expect(screen.getByText('Enter')).toBeInTheDocument()
    expect(screen.getAllByRole('tab').map((t) => t.getAttribute('aria-label'))).toEqual(['Say Hi!', 'Live Now', 'Add Friends', 'Make a Move'])
  })

  it('shows returning players their tips, skipping ones we have no feature for', () => {
    localStorage.setItem('launchCount', '2')
    countLaunch()
    render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(screen.getAllByRole('tab').map((t) => t.getAttribute('aria-label'))).toEqual(['Hang Out', 'Your People', 'Earn Badges', 'Live Now'])
  })

  it('follows the remote flag, and falls back to the generic tips without it', () => {
    resetFeatureFlags(flags({ newUsers: { displayed: ['make_move', 'say_hi'] } }))
    const { unmount } = render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(screen.getAllByRole('tab').map((t) => t.getAttribute('aria-label'))).toEqual(['Make a Move', 'Say Hi!'])
    unmount()
    resetFeatureFlags(flags())
    render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(screen.getAllByRole('tab')).toHaveLength(GENERIC_TIPS.length)
  })

  it('starts after the tip the last loading screen ended on', () => {
    localStorage.setItem('loadingLastTip', '1')
    render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(heading()).toBe('Add Friends')
  })

  it('rotates every 10s; the arrows restart the clock and the dots do not', () => {
    vi.useFakeTimers()
    render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    const wait = (ms: number): void => {
      act(() => vi.advanceTimersByTime(ms))
      act(() => vi.advanceTimersByTime(300))
    }
    wait(TIP_ROTATE_MS)
    expect(heading()).toBe('Live Now')
    act(() => vi.advanceTimersByTime(5000))
    fireEvent.click(screen.getByRole('button', { name: 'Next tip' }))
    wait(0)
    expect(heading()).toBe('Add Friends')
    wait(TIP_ROTATE_MS - 1000)
    expect(heading()).toBe('Add Friends')
    wait(1000)
    expect(heading()).toBe('Make a Move')
    act(() => vi.advanceTimersByTime(5000))
    fireEvent.click(screen.getAllByRole('tab')[0])
    wait(0)
    expect(heading()).toBe('Say Hi!')
    wait(TIP_ROTATE_MS - 5600)
    expect(heading()).toBe('Live Now')
  })

  it('shows the session progress, and RECONNECTING when the realm drops', () => {
    const { rerender } = render(<SceneLoadingOverlay scene={loading({ pendingAssets: 6 })} progress={52} />)
    expect(screen.getByText('LOADING 52%')).toBeInTheDocument()
    rerender(<SceneLoadingOverlay scene={loading({ realmConnected: false })} progress={52} />)
    expect(screen.getByText('RECONNECTING…')).toBeInTheDocument()
  })

  it('the progress fill starts 67px wide, like the reference bar', () => {
    render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(document.querySelector('[class*="fill"]')).toHaveStyle({ width: 'calc(0% + 67px)' })
  })

  it('shows the bug report button only while its feature flag is on', () => {
    const { unmount } = render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(screen.queryByRole('button', { name: 'Report a bug' })).toBeNull()
    unmount()
    resetFeatureFlags({ flags: { 'explorer-alfa-bug-report': true }, variants: {} })
    render(<SceneLoadingOverlay scene={loading()} progress={0} />)
    expect(screen.getByRole('button', { name: 'Report a bug' })).toBeInTheDocument()
  })
})
