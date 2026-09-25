import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'
import { resetProfileStore } from '../features/session/profileStore'

afterEach(() => {
  cleanup()
  resetProfileStore()
})

// jsdom doesn't implement these; components touch them on mount.
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver

if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addListener() {},
      removeListener() {},
      addEventListener() {},
      removeEventListener() {},
      dispatchEvent() {
        return false
      }
    }) as unknown as MediaQueryList
}

// Keep unit tests off the network: the sidebar polls the live events count on mount.
const realFetch = globalThis.fetch
globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) =>
  String(input instanceof Request ? input.url : input).includes('/api/events')
    ? Promise.resolve(new Response(JSON.stringify({ ok: true, data: [] })))
    : realFetch(input, init)) as typeof fetch
