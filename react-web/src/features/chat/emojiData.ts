// Emoji dataset ported from the SDK7 scene (emojis_complete.json). Emojis are
// plain Unicode inserted into the message string — no engine API involved. We use
// it for the grid picker and for ":shortcode:" autocomplete in the chat input.

import data from './emojis_complete.json'
import { getPref, PREF, setPref } from '../../lib/prefs'

export interface Emoji {
  code: string
  emoji: string
  /** ":grinning_face:" — colon-wrapped shortcode. */
  expression: string
  category: string
  subcategory: string
}

interface RawCategory {
  name: string
}

const ALL = data.emojis as Emoji[]

export interface EmojiGroup {
  name: string
  emojis: Emoji[]
}

export const EMOJI_GROUPS: EmojiGroup[] = (data.categories as RawCategory[]).map((c) => ({
  name: c.name,
  emojis: ALL.filter((e) => e.category === c.name)
}))

export const EMOJI_BY_CODE: Map<string, Emoji> = new Map(ALL.map((e) => [e.code, e]))
export const EMOJI_BY_GLYPH: Map<string, Emoji> = new Map(ALL.map((e) => [e.emoji, e]))

/** The reaction bar's fixed emoji, Unity's: ♥️ 👏 👍 👎 🤣 🔥 😢. */
export const QUICK_REACTIONS = ['\u2665\ufe0f', '\u{1f44f}', '\u{1f44d}', '\u{1f44e}', '\u{1f923}', '\u{1f525}', '\u{1f622}']

// "Frequently used" — persisted across sessions, most-recent first.
const RECENTS_MAX = 18

export function loadRecents(): string[] {
  try {
    const v = JSON.parse(getPref(PREF.emojiRecents) ?? '[]')
    return Array.isArray(v) ? (v as string[]) : []
  } catch {
    return []
  }
}

export function pushRecent(code: string): string[] {
  const next = [code, ...loadRecents().filter((c) => c !== code)].slice(0, RECENTS_MAX)
  setPref(PREF.emojiRecents, JSON.stringify(next))
  return next
}

/** Rank emoji whose shortcode starts-with then contains `query` (no colons). */
export function searchByShortcode(query: string, limit = 8): Emoji[] {
  const q = query.toLowerCase().replace(/^:/, '')
  if (!q) return []
  const starts: Emoji[] = []
  const contains: Emoji[] = []
  for (const e of ALL) {
    const name = e.expression.slice(1, -1)
    if (name.startsWith(q)) starts.push(e)
    else if (name.includes(q)) contains.push(e)
    if (starts.length >= limit) break
  }
  return [...starts, ...contains].slice(0, limit)
}
