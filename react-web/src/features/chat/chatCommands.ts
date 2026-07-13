// Chat slash-commands — parity with bevy-ui-scene's `sendChatMessage` (ChatsAndLogs.tsx) and
// unity-explorer's IChatCommand set. Pure parser: turns a raw input line into an action the session
// dispatches (teleport / changeRealm / reload / console) or a system message to echo in chat.
//
// `/goto` and `/world` reuse the existing teleport/changeRealm bridge plumbing; `/reload` and
// `/commands` go through new thin bridge handlers (reloadScene / consoleCommand). `/help` is a
// client-rendered system message; `/commands` surfaces the engine's own console command list.

/** The DCL default (Genesis) realm — `/goto genesis` / `/goto main` target this. */
const GENESIS_ALIASES = new Set(['genesis', 'main'])

export type ChatCommand =
  /** Not a command (no leading `/`) — send as a normal chat message. */
  | { kind: 'send'; text: string }
  /** `/goto x,y` — teleport to a parcel. */
  | { kind: 'goto'; x: number; y: number }
  /** `/goto genesis|main` — change to the default (Genesis) realm. */
  | { kind: 'genesis' }
  /** `/goto <world>` or `/world <world>` — jump to a world's realm (normalized to `.dcl.eth`). */
  | { kind: 'world'; realm: string }
  /** `/reload` — reload the current scene. */
  | { kind: 'reload' }
  /** `/commands` — list the engine console commands. */
  | { kind: 'commands' }
  /** `/help` (and invalid usage) — echo this text as a system message. */
  | { kind: 'system'; message: string }

/** The `/help` body — the client (DCL) commands, plain text (chat has no bold). */
export const HELP_TEXT = [
  'Available commands:',
  '/help — show this help',
  '/goto x,y — teleport to parcel x,y',
  '/goto <world> — jump to a world (e.g. world_name or world_name.dcl.eth)',
  '/goto genesis — go to Genesis Plaza',
  '/world <world> — jump to a world (alias of /goto <world>)',
  '/reload — reload the current scene',
  '/commands — list the engine console commands',
].join('\n')

/** Normalize a world token to its ENS realm: `boedo` → `boedo.dcl.eth`; `foo.eth` stays. */
function toRealm(token: string): string {
  return token.includes('.eth') ? token : `${token.replace('.dcl.eth', '')}.dcl.eth`
}

const COORDS_RE = /^(-?\d+)\s*,\s*(-?\d+)$/

// `/goto` and `/world` share the realm/coords parsing; only `/goto` accepts coordinates.
function parseGoto(rest: string, allowCoords: boolean): ChatCommand {
  const arg = rest.trim()
  if (!arg) return { kind: 'system', message: 'Usage: /goto x,y  ·  /goto <world>  ·  /goto genesis' }
  if (GENESIS_ALIASES.has(arg.toLowerCase())) return { kind: 'genesis' }
  const m = allowCoords ? arg.match(COORDS_RE) : null
  if (m) return { kind: 'goto', x: Number(m[1]), y: Number(m[2]) }
  // A single token → world name; anything with a space/comma that isn't coords is invalid.
  if (/\s|,/.test(arg)) return { kind: 'system', message: `Invalid destination: ${arg}` }
  return { kind: 'world', realm: toRealm(arg) }
}

/** Parse a raw chat input into an action. Non-`/` lines pass through as `send`. */
export function parseChatCommand(input: string): ChatCommand {
  const text = input.trim()
  if (!text.startsWith('/')) return { kind: 'send', text }

  const [word, ...restParts] = text.split(/\s+/)
  const rest = text.slice(word.length).trim()
  switch (word.toLowerCase()) {
    case '/help':
      return { kind: 'system', message: HELP_TEXT }
    case '/commands':
      return { kind: 'commands' }
    case '/reload':
      return { kind: 'reload' }
    case '/goto':
      return parseGoto(rest, true)
    case '/world':
      return restParts.length === 0
        ? { kind: 'system', message: 'Usage: /world <world>' }
        : parseGoto(rest, false)
    default:
      return { kind: 'system', message: `Unknown command ${word}. Type /help for the list.` }
  }
}
