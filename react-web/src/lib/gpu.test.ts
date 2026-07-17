import { afterEach, describe, expect, it, vi } from 'vitest'
import { hasUsableGpu } from './gpu'

// jsdom's navigator has no `gpu`; define/override it per case, restore after.
const original = (navigator as unknown as { gpu?: unknown }).gpu
function setGpu(gpu: unknown): void {
  Object.defineProperty(navigator, 'gpu', { value: gpu, configurable: true, writable: true })
}
afterEach(() => {
  setGpu(original)
  vi.useRealTimers()
})

describe('hasUsableGpu', () => {
  it('is false when WebGPU is absent (navigator.gpu undefined)', async () => {
    setGpu(undefined)
    expect(await hasUsableGpu()).toBe(false)
  })

  it('is false when requestAdapter resolves null (no usable GPU)', async () => {
    setGpu({ requestAdapter: () => Promise.resolve(null) })
    expect(await hasUsableGpu()).toBe(false)
  })

  it('is true when requestAdapter resolves an adapter', async () => {
    setGpu({ requestAdapter: () => Promise.resolve({}) })
    expect(await hasUsableGpu()).toBe(true)
  })

  it('is false when requestAdapter throws', async () => {
    setGpu({ requestAdapter: () => Promise.reject(new Error('boom')) })
    expect(await hasUsableGpu()).toBe(false)
  })

  it('fails open (true) when the probe hangs past the timeout', async () => {
    vi.useFakeTimers()
    setGpu({ requestAdapter: () => new Promise(() => {}) }) // never resolves
    const result = hasUsableGpu()
    await vi.advanceTimersByTimeAsync(5000)
    expect(await result).toBe(true)
  })
})
