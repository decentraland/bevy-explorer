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
  /** `/goto x,y` — teleport to a parcel of the realm the player is already in. */
  | { kind: 'goto'; x: number; y: number }
  /** `/goto genesis|main [x,y]` — go to Genesis Plaza. Defaults to its base parcel (0,0); an
   *  explicit x,y overrides it (testing other Genesis parcels — see backlog 45). */
  | { kind: 'genesis'; x?: number; y?: number }
  /** `/goto <world> [x,y]` or `/world <world> [x,y]` — jump to a world's realm (normalized to
   *  `.dcl.eth`). With x,y, teleports there once the realm is live; without, the world's own
   *  default spawn applies. */
  | { kind: 'world'; realm: string; x?: number; y?: number }
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
  '/goto <world> [x,y] — jump to a world, optionally at parcel x,y (e.g. world_name or world_name.dcl.eth)',
  '/goto genesis [x,y] — go to Genesis Plaza, optionally at parcel x,y',
  '/reload — reload the current scene',
  '/commands — list the engine console commands',
].join('\n')

/** Normalize a world token to its ENS realm: `boedo` → `boedo.dcl.eth`; `foo.eth` stays. */
function toRealm(token: string): string {
  return token.includes('.eth') ? token : `${token.replace('.dcl.eth', '')}.dcl.eth`
}

const COORDS_RE = /^(-?\d+)\s*,\s*(-?\d+)$/

// `/goto` and `/world` share the realm/coords parsing; only `/goto` accepts bare coordinates
// (`/goto x,y`, no destination — teleport within the current realm). Both accept an optional
// trailing `x,y` after a destination (`/goto <world|genesis> x,y`) to target a specific parcel
// there instead of the destination's default.
function parseGoto(rest: string, allowCoords: boolean): ChatCommand {
  const trimmed = rest.trim()
  if (!trimmed) {
    return { kind: 'system', message: 'Usage: /goto x,y  ·  /goto <world> [x,y]  ·  /goto genesis [x,y]' }
  }

  // Bare coords (tolerates "x, y" with a space after the comma), checked against the whole
  // remainder before any tokenizing — a trailing destination coords pair below is checked the
  // same way, just against the substring after the destination token.
  if (allowCoords) {
    const m = trimmed.match(COORDS_RE)
    if (m) return { kind: 'goto', x: Number(m[1]), y: Number(m[2]) }
  }

  // <destination> [x,y] — split on the FIRST run of whitespace only, so the trailing coords
  // argument can itself contain "x, y" with a space after the comma.
  const firstSpace = trimmed.search(/\s/)
  const first = firstSpace === -1 ? trimmed : trimmed.slice(0, firstSpace)
  const remainder = firstSpace === -1 ? '' : trimmed.slice(firstSpace).trim()

  let coords: { x: number; y: number } | undefined
  if (remainder) {
    const m = remainder.match(COORDS_RE)
    if (!m) return { kind: 'system', message: `Invalid destination: ${trimmed}` }
    coords = { x: Number(m[1]), y: Number(m[2]) }
  }

  if (GENESIS_ALIASES.has(first.toLowerCase())) return { kind: 'genesis', ...coords }
  // A coords-shaped or otherwise invalid first token isn't a destination name.
  if (/\s|,/.test(first)) return { kind: 'system', message: `Invalid destination: ${trimmed}` }
  return { kind: 'world', realm: toRealm(first), ...coords }
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
