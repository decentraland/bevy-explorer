import { describe, expect, it } from 'vitest'
import { parseChatCommand, HELP_TEXT } from './chatCommands'

describe('parseChatCommand', () => {
  it('passes a normal message through as send (trimmed)', () => {
    expect(parseChatCommand('hello world')).toEqual({ kind: 'send', text: 'hello world' })
    expect(parseChatCommand('  hi  ')).toEqual({ kind: 'send', text: 'hi' })
    // A slash mid-word is not a command.
    expect(parseChatCommand('and/or')).toEqual({ kind: 'send', text: 'and/or' })
  })

  it('/help echoes the help text as a system message', () => {
    const r = parseChatCommand('/help')
    expect(r.kind).toBe('system')
    expect(r).toEqual({ kind: 'system', message: HELP_TEXT })
    // The help lists /commands and the goto forms.
    expect(HELP_TEXT).toContain('/commands')
    expect(HELP_TEXT).toContain('/goto')
  })

  it('/commands and /reload map to their actions', () => {
    expect(parseChatCommand('/commands')).toEqual({ kind: 'commands' })
    expect(parseChatCommand('/reload')).toEqual({ kind: 'reload' })
  })

  it('/goto x,y parses coordinates (tolerating spaces and negatives)', () => {
    expect(parseChatCommand('/goto 10,20')).toEqual({ kind: 'goto', x: 10, y: 20 })
    expect(parseChatCommand('/goto -5, 3')).toEqual({ kind: 'goto', x: -5, y: 3 })
  })

  it('/goto genesis|main → genesis realm', () => {
    expect(parseChatCommand('/goto genesis')).toEqual({ kind: 'genesis' })
    expect(parseChatCommand('/goto main')).toEqual({ kind: 'genesis' })
    expect(parseChatCommand('/GOTO Genesis')).toEqual({ kind: 'genesis' })
  })

  it('/goto <world> normalizes to a .dcl.eth realm', () => {
    expect(parseChatCommand('/goto boedo')).toEqual({ kind: 'world', realm: 'boedo.dcl.eth' })
    expect(parseChatCommand('/goto boedo.dcl.eth')).toEqual({ kind: 'world', realm: 'boedo.dcl.eth' })
    // A bare .eth name is left as-is.
    expect(parseChatCommand('/goto foo.eth')).toEqual({ kind: 'world', realm: 'foo.eth' })
  })

  it('/world behaves like /goto <world> but takes no bare coordinates', () => {
    expect(parseChatCommand('/world boedo')).toEqual({ kind: 'world', realm: 'boedo.dcl.eth' })
    expect(parseChatCommand('/world genesis')).toEqual({ kind: 'genesis' })
  })

  // A trailing x,y after a destination targets a specific parcel there instead of its default
  // spawn — added for testing the realm-arrival-wait/stale-pointer behavior (backlog 45).
  it('/goto <world> x,y and /goto genesis x,y carry the override parcel', () => {
    expect(parseChatCommand('/goto pablo 3,1')).toEqual({ kind: 'world', realm: 'pablo.dcl.eth', x: 3, y: 1 })
    expect(parseChatCommand('/goto main -3,-2')).toEqual({ kind: 'genesis', x: -3, y: -2 })
    expect(parseChatCommand('/goto genesis 0,0')).toEqual({ kind: 'genesis', x: 0, y: 0 })
    // /world shares the same trailing-coords parsing as /goto.
    expect(parseChatCommand('/world pablo 3,1')).toEqual({ kind: 'world', realm: 'pablo.dcl.eth', x: 3, y: 1 })
  })

  it('bare /goto <world> and /goto genesis (no coords) carry no x,y', () => {
    const world = parseChatCommand('/goto pablo')
    expect(world).toEqual({ kind: 'world', realm: 'pablo.dcl.eth' })
    expect(world).not.toHaveProperty('x')
    const genesis = parseChatCommand('/goto main')
    expect(genesis).toEqual({ kind: 'genesis' })
    expect(genesis).not.toHaveProperty('x')
  })

  it('an invalid trailing arg after a destination is a system message, never a broadcast', () => {
    expect(parseChatCommand('/goto pablo notcoords').kind).toBe('system')
    expect(parseChatCommand('/goto main 3').kind).toBe('system')
  })

  it('missing/invalid args → a system usage message, never a broadcast', () => {
    expect(parseChatCommand('/goto').kind).toBe('system')
    expect(parseChatCommand('/world').kind).toBe('system')
    expect(parseChatCommand('/goto 1 2 3').kind).toBe('system')
  })

  it('an unknown /command → a system hint, not a broadcast', () => {
    const r = parseChatCommand('/dance')
    expect(r.kind).toBe('system')
    expect((r as { message: string }).message).toContain('/help')
  })
})
