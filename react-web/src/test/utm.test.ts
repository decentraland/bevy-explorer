import { describe, it, expect } from 'vitest'
import { withUtm } from '../lib/utm'

describe('utm source', () => {
  it('tags links from the web build', () => {
    expect(withUtm('https://decentraland.org/shop')).toBe('https://decentraland.org/shop?utm_source=bevy-web-client')
    expect(withUtm('https://decentraland.org/shop?x=1')).toBe('https://decentraland.org/shop?x=1&utm_source=bevy-web-client')
  })
})
