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

  it('/world behaves like /goto <world> but takes no coordinates', () => {
    expect(parseChatCommand('/world boedo')).toEqual({ kind: 'world', realm: 'boedo.dcl.eth' })
    expect(parseChatCommand('/world genesis')).toEqual({ kind: 'genesis' })
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
