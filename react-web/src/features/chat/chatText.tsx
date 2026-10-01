// Rich chat text — ports the SDK7 chat's `decorateMessageWithLinks`: turn a raw
// message into clickable URLs, location coords (teleport), and @username mentions
// (clickable → profile viewer; highlighted when they mention you). Parsing is a pure
// function so it's unit-testable; <MessageText> renders the tokens with handlers.

import { mentionName, mentionsName } from '../../engine/mention'
import type { NearbyMember } from '../../engine/protocol'

export type Token =
  | { type: 'text'; value: string }
  | { type: 'url'; value: string }
  | { type: 'world'; value: string }
  | { type: 'location'; value: string; x: number; y: number }
  | { type: 'mention'; value: string; name: string; tag?: string }

// URL → world name → location (x,y) → @mention, scanned in one pass to keep original order.
// Worlds are ENS names (e.g. boedo.dcl.eth) → clickable "jump to realm". Coords require both
// signs/commas so we don't linkify every number; mentions allow an optional #tag suffix
// (Name#a1b2) like the engine's claimed-name disambiguation.
const TOKEN_RE =
  /(?<url>https?:\/\/[^\s<>"']+)|(?<world>[a-z0-9][\w-]*\.(?:dcl\.)?eth\b)|(?<loc>-?\d{1,3}\s*,\s*-?\d{1,3})|(?<mention>@[\w-]+(?:#[\w]+)?)/gi

export function parseMessage(text: string): Token[] {
  const tokens: Token[] = []
  let last = 0
  for (const m of text.matchAll(TOKEN_RE)) {
    const i = m.index ?? 0
    if (i > last) tokens.push({ type: 'text', value: text.slice(last, i) })
    const g = m.groups ?? {}
    if (g.url) {
      tokens.push({ type: 'url', value: g.url })
    } else if (g.world) {
      tokens.push({ type: 'world', value: g.world })
    } else if (g.loc) {
      const [x, y] = g.loc.split(',').map((s) => parseInt(s.trim(), 10))
      tokens.push({ type: 'location', value: g.loc, x, y })
    } else if (g.mention) {
      const [name, tag] = g.mention.slice(1).split('#')
      tokens.push({ type: 'mention', value: g.mention, name, tag })
    }
    last = i + m[0].length
  }
  if (last < text.length) tokens.push({ type: 'text', value: text.slice(last) })
  return tokens
}

/** Lowercased mention name (Name, or Name#1a2b when unclaimed) → address, from the nearby roster. */
export function buildNameIndex(members: NearbyMember[]): Map<string, string> {
  const idx = new Map<string, string>()
  for (const m of members) {
    if (!m.name.trim()) continue
    const mention = mentionName(m.name, m.address, m.claimed).toLowerCase()
    idx.set(mention, m.address)
    // A bare @Name still resolves to an unclaimed Name#1a2b when it's the only one nearby.
    const base = mention.split('#')[0]
    if (!idx.has(base)) idx.set(base, m.address)
  }
  return idx
}

function resolveMention(t: Extract<Token, { type: 'mention' }>, index: Map<string, string>): string | undefined {
  return t.tag ? index.get(`${t.name}#${t.tag}`.toLowerCase()) : index.get(t.name.toLowerCase())
}

/** Does this message @-mention me? Exactly my mention name, so a different Name#ffff doesn't count. */
export function mentionsMe(text: string, me: { address?: string; name?: string; hasClaimedName?: boolean } | null): boolean {
  if (!me?.name || !me.address) return false
  return mentionsName(text, mentionName(me.name, me.address, me.hasClaimedName))
}

export function MessageText({
  text,
  members,
  styles,
  onMention,
  onLocation,
  onWorld
}: {
  text: string
  members: NearbyMember[]
  styles: { url: string; mention: string; location: string; world: string }
  /** A resolved @mention was clicked (address known). */
  onMention: (address: string, name: string, e: React.MouseEvent) => void
  /** A location link (x,y) was clicked. */
  onLocation: (x: number, y: number) => void
  /** A world name (e.g. boedo.dcl.eth) was clicked → prompt to jump to that realm. */
  onWorld?: (name: string) => void
}): React.JSX.Element {
  const index = buildNameIndex(members)
  return (
    <>
      {parseMessage(text).map((t, i) => {
        if (t.type === 'url') {
          return (
            <a key={i} className={styles.url} href={t.value} target="_blank" rel="noreferrer noopener">
              {t.value}
            </a>
          )
        }
        if (t.type === 'world') {
          if (!onWorld) return <span key={i}>{t.value}</span>
          return (
            <button key={i} type="button" className={styles.world} onClick={() => onWorld(t.value)}>
              {t.value}
            </button>
          )
        }
        if (t.type === 'location') {
          return (
            <button key={i} type="button" className={styles.location} onClick={() => onLocation(t.x, t.y)}>
              {t.value}
            </button>
          )
        }
        if (t.type === 'mention') {
          const addr = resolveMention(t, index)
          if (!addr) return <span key={i}>{t.value}</span>
          return (
            <button
              key={i}
              type="button"
              className={styles.mention}
              onClick={(e) => onMention(addr, t.name, e)}
              onContextMenu={(e) => onMention(addr, t.name, e)}
            >
              @{t.name}
            </button>
          )
        }
        return <span key={i}>{t.value}</span>
      })}
    </>
  )
}
