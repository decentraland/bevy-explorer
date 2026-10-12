import { describe, it, expect, vi, afterEach } from 'vitest'
import { relayStream } from '../../bridge-scene/src/relay'

async function* items<T>(list: T[], thenThrow = false): AsyncIterable<T> {
  for (const x of list) yield x
  if (thenThrow) throw new Error('stream broke')
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await Promise.resolve()
}

afterEach(() => vi.restoreAllMocks())

describe('bridge stream relay', () => {
  it('keeps relaying after one item throws', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    const seen: number[] = []
    let stop = false
    relayStream('t', async () => items([1, 2, 3]), (n) => {
      if (n === 2) throw new Error('bad item')
      seen.push(n)
    }, () => new Promise(() => {}), () => stop)
    await flush()
    stop = true
    expect(seen).toEqual([1, 3])
  })

  it('reopens the stream after it fails or ends', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {})
    const seen: string[] = []
    const opens = vi.fn<() => Promise<AsyncIterable<string>>>()
      .mockImplementationOnce(async () => items(['a'], true))
      .mockImplementationOnce(async () => items(['b']))
      .mockImplementation(async () => items<string>([]))
    const delays: number[] = []
    let stop = false
    relayStream('t', opens, (s) => { seen.push(s) }, async (ms) => {
      delays.push(ms)
      if (delays.length >= 3) stop = true
    }, () => stop)
    await flush()
    expect(seen).toEqual(['a', 'b'])
    expect(opens).toHaveBeenCalledTimes(3)
    expect(delays[0]).toBe(1000)
  })
})
