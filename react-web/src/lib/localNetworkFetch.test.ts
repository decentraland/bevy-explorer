import { afterEach, describe, expect, it, vi } from 'vitest'
import { installLocalNetworkFetch, targetAddressSpaceOf, type LnaRequestInit } from './localNetworkFetch'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('targetAddressSpaceOf', () => {
  it.each([
    ['http://localhost:8000/about', 'loopback'],
    ['http://127.0.0.1:1234/content', 'loopback'],
    ['http://[::1]:8000/', 'loopback'],
    ['http://10.0.0.5/x', 'local'],
    ['http://192.168.1.10:2044/', 'local'],
    ['http://172.16.0.1/', 'local'],
    ['http://169.254.1.1/', 'local'],
    ['http://gaming-rig.local/', 'local'],
    ['http://172.32.0.1/', null], // outside 172.16/12 — public
    ['https://decentraland.org/bevy-web/', null],
    ['https://peer.decentraland.org/content', null]
  ])('%s → %s', (url, expected) => {
    expect(targetAddressSpaceOf(url)).toBe(expected)
  })

  it('resolves relative URLs against the page origin', () => {
    // jsdom serves tests from http://localhost — relative is loopback here.
    expect(targetAddressSpaceOf('/engine/pkg')).toBe('loopback')
  })

  it('returns null for unparseable input instead of throwing', () => {
    expect(targetAddressSpaceOf('http://')).toBe(null)
  })
})

describe('installLocalNetworkFetch', () => {
  function install(): ReturnType<typeof vi.fn> {
    const mock = vi.fn().mockResolvedValue(new Response())
    vi.stubGlobal('fetch', mock)
    installLocalNetworkFetch()
    return mock
  }

  it('annotates loopback string URLs with targetAddressSpace', async () => {
    const mock = install()
    await window.fetch('http://127.0.0.1:8000/about')
    expect(mock).toHaveBeenCalledWith('http://127.0.0.1:8000/about', { targetAddressSpace: 'loopback' })
  })

  it('annotates Request-object inputs via the init override', async () => {
    const mock = install()
    const req = new Request('http://localhost:2044/content/contents/x')
    await window.fetch(req)
    expect(mock).toHaveBeenCalledWith(req, { targetAddressSpace: 'loopback' })
  })

  it('preserves existing init options', async () => {
    const mock = install()
    await window.fetch('http://192.168.1.4:2044/about', { method: 'HEAD' })
    expect(mock).toHaveBeenCalledWith('http://192.168.1.4:2044/about', {
      method: 'HEAD',
      targetAddressSpace: 'local'
    })
  })

  it('leaves public URLs untouched', async () => {
    const mock = install()
    await window.fetch('https://peer.decentraland.org/about')
    expect(mock).toHaveBeenCalledWith('https://peer.decentraland.org/about', undefined)
  })

  it('respects a caller-provided targetAddressSpace', async () => {
    const mock = install()
    const init: LnaRequestInit = { targetAddressSpace: 'local' }
    await window.fetch('http://127.0.0.1:8000/about', init)
    expect(mock).toHaveBeenCalledWith('http://127.0.0.1:8000/about', init)
  })
})
