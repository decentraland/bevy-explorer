import { describe, it, expect } from 'vitest'
import { parseMessage, buildNameIndex, mentionsMe } from '../features/chat/chatText'

describe('parseMessage', () => {
  it('keeps plain text as a single token', () => {
    expect(parseMessage('hello world')).toEqual([{ type: 'text', value: 'hello world' }])
  })

  it('extracts a URL in order', () => {
    expect(parseMessage('see https://dcl.org/x ok')).toEqual([
      { type: 'text', value: 'see ' },
      { type: 'url', value: 'https://dcl.org/x' },
      { type: 'text', value: ' ok' }
    ])
  })

  it('extracts a location coord with parsed x/y', () => {
    expect(parseMessage('go 10,-5 now')).toEqual([
      { type: 'text', value: 'go ' },
      { type: 'location', value: '10,-5', x: 10, y: -5 },
      { type: 'text', value: ' now' }
    ])
  })

  it('extracts an @mention (with optional #tag)', () => {
    expect(parseMessage('hi @Alice!')).toEqual([
      { type: 'text', value: 'hi ' },
      { type: 'mention', value: '@Alice', name: 'Alice', tag: undefined },
      { type: 'text', value: '!' }
    ])
    expect(parseMessage('yo @Bob#12ab')[1]).toEqual({ type: 'mention', value: '@Bob#12ab', name: 'Bob', tag: '12ab' })
  })

  it('extracts a world name (.dcl.eth and bare .eth)', () => {
    expect(parseMessage('join boedo.dcl.eth now')).toEqual([
      { type: 'text', value: 'join ' },
      { type: 'world', value: 'boedo.dcl.eth' },
      { type: 'text', value: ' now' }
    ])
    expect(parseMessage('go name.eth')[1]).toEqual({ type: 'world', value: 'name.eth' })
  })

  it('does not break a URL that contains a world name', () => {
    expect(parseMessage('https://play.decentraland.org/?realm=boedo.dcl.eth')).toEqual([
      { type: 'url', value: 'https://play.decentraland.org/?realm=boedo.dcl.eth' }
    ])
  })

  it('handles a message mixing all three', () => {
    const t = parseMessage('@Alice come to 5,5 see https://x.io')
    expect(t.map((x) => x.type)).toEqual(['mention', 'text', 'location', 'text', 'url'])
  })
})

describe('buildNameIndex + mentionsMe', () => {
  const ALICE = '0x00000000000000000000000000000000000a1b2c'
  const members = [
    { address: '0x00000000000000000000000000000000000000aa', name: 'Me', claimed: true },
    { address: ALICE, name: 'Alice', claimed: false }
  ]

  it('indexes an unclaimed name with its wallet tag, and bare while unambiguous', () => {
    const idx = buildNameIndex(members)
    expect(idx.get('me')).toBe(members[0].address)
    expect(idx.get('alice#1b2c')).toBe(ALICE)
    expect(idx.get('alice')).toBe(ALICE)
  })

  it('a claimed name is mentioned by name alone', () => {
    const me = { address: '0x00000000000000000000000000000000000000aa', name: 'Me', hasClaimedName: true }
    expect(mentionsMe('hey @Me!', me)).toBe(true)
    expect(mentionsMe('hey @Alice', me)).toBe(false)
  })

  it('an unclaimed name is mentioned only with its own wallet tag', () => {
    const me = { address: ALICE, name: 'Alice#1b2c', hasClaimedName: false }
    expect(mentionsMe('hi @Alice#1b2c', me)).toBe(true)
    expect(mentionsMe('hi @Alice#ffff', me)).toBe(false)
  })
})
