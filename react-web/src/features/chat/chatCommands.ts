// Chat slash-commands — parity with bevy-ui-scene's `sendChatMessage` (ChatsAndLogs.tsx) and
// unity-explorer's IChatCommand set. Pure parser: turns a raw input line into an action the session
// dispatches (teleport / changeRealm / reload / console) or a system message to echo in chat.
//
// `/goto` and `/world` reuse the existing teleport/changeRealm bridge plumbing; `/reload` and
// `/commands` go through thin bridge handlers (reloadScene / consoleCommand). `/help` is a
// client-rendered system message; `/commands` surfaces the engine's own console command list.
// Any other `/word args` is passed through to the engine console (`consoleCommand`), except the
// HUD_HIDDEN_COMMANDS below; the engine itself only registers dev/cheat commands in preview mode.

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
  /** Any other `/command args` — run it on the engine console (no leading slash, args pre-split). */
  | { kind: 'console'; command: string; args: string[] }
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
  '/<command> [args] — run any engine console command (see /commands; /help <command> for its usage)',
].join('\n')

/** Engine console commands the HUD never runs from chat, and drops from `/commands`, even though
 *  the engine registers them: the scene-inspector/editor set serves other HUDs (the editor and
 *  component-inspector system scenes) through the same console op and dumps whole-scene JSON;
 *  lock/unlock_preview and show_ui are preview-scene tooling; the login/logout/chat set are
 *  agent-harness commands that would desync this HUD's own session state; clear/exit are a
 *  console no-op and a native quit. */
export const HUD_HIDDEN_COMMANDS: ReadonlySet<string> = new Set([
  // scene inspector — read
  'set_scene',
  'scene_stats',
  'scene_target',
  'scene_logs',
  'scene_entities',
  'entity_components',
  'inspect_component',
  'scene_tree',
  'crdt_snapshot',
  'crdt_initial',
  'component_names',
  'component_default',
  'component_schema',
  // scene inspector — write / assets
  'set_component',
  'set_component_raw',
  'new_entity',
  'save_composite',
  'delete_component',
  'delete_entity',
  'freeze_scene',
  'unfreeze_scene',
  'tick_scene',
  'highlight',
  'scene_content',
  'asset_catalog',
  'init_asset',
  // preview-scene tooling
  'lock_preview',
  'unlock_preview',
  'show_ui',
  // session / agent harness
  'login_guest',
  'login_previous',
  'login_identity',
  'logout',
  'chat',
  // console no-op / native quit
  'clear',
  'exit',
])

/** Split a console argument string on whitespace, honouring "double" and 'single' quotes. */
export function splitArgs(rest: string): string[] {
  const out: string[] = []
  const re = /"([^"]*)"|'([^']*)'|(\S+)/g
  let m: RegExpExecArray | null
  while ((m = re.exec(rest)) != null) out.push(m[1] ?? m[2] ?? m[3])
  return out
}

/** Render an engine console reply as a chat system line. A bare `help` (`/commands`) drops the
 *  HUD-hidden commands; the engine's unknown-command rejection is cut to its first sentence (its
 *  "Recognized commands: [...]" tail is the whole registry). */
export function formatConsoleReply(command: string, args: string[], output: string): string {
  if (command === 'help' && args.length === 0) {
    return output
      .split('\n')
      .filter((line) => {
        const m = line.match(/^\s*\/(\S+)/)
        return m == null || !HUD_HIDDEN_COMMANDS.has(m[1])
      })
      .join('\n')
      .trim()
  }
  const unknown = output.match(/^Command not recognized: `[^`]*`/)
  if (unknown != null) return `${unknown[0]}. Type /commands for the list.`
  return output.trim() || `(no output for /${command})`
}

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
  const name = word.slice(1).toLowerCase()
  switch (name) {
    case 'help':
      // `/help <command>` → the engine's per-command usage (its registry keys carry the slash).
      if (restParts.length > 0) return { kind: 'console', command: 'help', args: [`/${restParts[0].replace(/^\//, '')}`] }
      return { kind: 'system', message: HELP_TEXT }
    case 'commands':
      return { kind: 'commands' }
    case 'reload':
      // Bare `/reload` targets the player's scene (the engine's bare `/reload` reloads every scene,
      // this HUD's bridge included); with a hash it is the engine command.
      if (restParts.length > 0) return { kind: 'console', command: 'reload', args: splitArgs(rest) }
      return { kind: 'reload' }
    case 'goto':
      return parseGoto(rest, true)
    case 'world':
      return restParts.length === 0
        ? { kind: 'system', message: 'Usage: /world <world>' }
        : parseGoto(rest, false)
    default:
      if (name === '' || HUD_HIDDEN_COMMANDS.has(name)) {
        return { kind: 'system', message: `Unknown command ${word}. Type /help for the list.` }
      }
      return { kind: 'console', command: name, args: splitArgs(rest) }
  }
}
