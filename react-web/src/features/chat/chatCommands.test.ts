import { describe, expect, it } from 'vitest'
import { formatConsoleReply, HUD_HIDDEN_COMMANDS, parseChatCommand, splitArgs, HELP_TEXT } from './chatCommands'

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

  it('any other /command passes through to the engine console with split args', () => {
    expect(parseChatCommand('/dance')).toEqual({ kind: 'console', command: 'dance', args: [] })
    expect(parseChatCommand('/Emote 3')).toEqual({ kind: 'console', command: 'emote', args: ['3'] })
    expect(parseChatCommand('/changerealm boedo.dcl.eth "https://x.y/content"')).toEqual({
      kind: 'console',
      command: 'changerealm',
      args: ['boedo.dcl.eth', 'https://x.y/content']
    })
    // A bare slash is not a command.
    expect(parseChatCommand('/').kind).toBe('system')
  })

  it('HUD-hidden engine commands → the unknown-command hint, never the console', () => {
    for (const name of ['crdt_snapshot', 'screenshot', 'set_component', 'lock_preview', 'logout', 'exit']) {
      expect(HUD_HIDDEN_COMMANDS.has(name)).toBe(true)
      const r = parseChatCommand(`/${name} 1 2`)
      expect(r.kind).toBe('system')
      expect((r as { message: string }).message).toContain('/help')
    }
  })

  it('/reload <hash> and /help <command> go to the engine; the bare forms stay local', () => {
    expect(parseChatCommand('/reload bafyabc')).toEqual({ kind: 'console', command: 'reload', args: ['bafyabc'] })
    expect(parseChatCommand('/help teleport')).toEqual({ kind: 'console', command: 'help', args: ['/teleport'] })
    expect(parseChatCommand('/help /teleport')).toEqual({ kind: 'console', command: 'help', args: ['/teleport'] })
    expect(parseChatCommand('/help').kind).toBe('system')
  })

  it('splitArgs honours quotes', () => {
    expect(splitArgs(`a "b c" 'd e' f`)).toEqual(['a', 'b c', 'd e', 'f'])
    expect(splitArgs('')).toEqual([])
  })
})

describe('formatConsoleReply', () => {
  it('drops HUD-hidden commands from the bare help listing', () => {
    const out = ['Available commands:', '  /teleport      - set location', '  /crdt_snapshot - Return the full live CRDT state', '  /emote         - emote'].join('\n')
    const shown = formatConsoleReply('help', [], out)
    expect(shown).toContain('/teleport')
    expect(shown).toContain('/emote')
    expect(shown).not.toContain('/crdt_snapshot')
    // Per-command help is passed through untouched.
    expect(formatConsoleReply('help', ['/teleport'], 'set location\n\nUsage: /teleport <X> <Y>')).toContain('Usage:')
  })

  it("trims the engine's unknown-command rejection to one sentence", () => {
    const rejection = 'Command not recognized: `/dance`. Recognized commands: ["/clear", "/emote", "/exit"]'
    expect(formatConsoleReply('dance', [], rejection)).toBe('Command not recognized: `/dance`. Type /commands for the list.')
  })

  it('reports empty output explicitly', () => {
    expect(formatConsoleReply('fps', ['60'], '  ')).toBe('(no output for /fps)')
    expect(formatConsoleReply('fps', ['60'], 'fps set to 60\n')).toBe('fps set to 60')
  })
})
