import { describe, it, expect, vi } from 'vitest'
import { readAllPages } from '../engine/paging'

// Audit backpack-emotes-11: owned emotes were one request of 200, so collectors lost the rest.
describe('readAllPages', () => {
  const items = Array.from({ length: 450 }, (_, i) => i)
  const api = vi.fn(async (pageNum: number) => ({ elements: items.slice((pageNum - 1) * 200, pageNum * 200), totalAmount: 450 }))

  it('keeps reading until totalAmount is reached', async () => {
    expect(await readAllPages(api, { pageSize: 200, maxPages: 25 })).toHaveLength(450)
    expect(api).toHaveBeenCalledTimes(3)
  })

  it('stops at maxPages', async () => {
    expect(await readAllPages(api, { pageSize: 200, maxPages: 2 })).toHaveLength(400)
  })

  it('rejects when a page fails instead of returning a short list', async () => {
    const flaky = vi.fn(async (pageNum: number) => {
      if (pageNum === 2) throw new Error('HTTP 503')
      return { elements: items.slice(0, 200), totalAmount: 450 }
    })
    await expect(readAllPages(flaky, { pageSize: 200, maxPages: 25 })).rejects.toThrow('HTTP 503')
  })
})
