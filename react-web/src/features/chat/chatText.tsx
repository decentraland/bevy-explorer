// Rich chat text — ports the SDK7 chat's `decorateMessageWithLinks`: turn a raw
// message into clickable URLs, location coords (teleport), and @username mentions
// (clickable → profile viewer; highlighted when they mention you). Parsing is a pure
// function so it's unit-testable; <MessageText> renders the tokens with handlers.

import type { NearbyMember } from '../../engine/protocol'

export type Token =
  | { type: 'text'; value: string }
  | { type: 'url'; value: string }
  | { type: 'location'; value: string; x: number; y: number }
  | { type: 'mention'; value: string; name: string; tag?: string }

// URL → location (x,y) → @mention, scanned in one pass to keep original order.
// Coords require both signs/commas so we don't linkify every number; mentions allow
// an optional #tag suffix (Name#a1b2) like the engine's claimed-name disambiguation.
const TOKEN_RE =
  /(?<url>https?:\/\/[^\s<>"']+)|(?<loc>-?\d{1,3}\s*,\s*-?\d{1,3})|(?<mention>@[\w-]+(?:#[\w]+)?)/g

export function parseMessage(text: string): Token[] {
  const tokens: Token[] = []
  let last = 0
  for (const m of text.matchAll(TOKEN_RE)) {
    const i = m.index ?? 0
    if (i > last) tokens.push({ type: 'text', value: text.slice(last, i) })
    const g = m.groups ?? {}
    if (g.url) {
      tokens.push({ type: 'url', value: g.url })
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

/** Lowercased name (and name#tag) → address, from the nearby roster. */
export function buildNameIndex(members: NearbyMember[]): Map<string, string> {
  const idx = new Map<string, string>()
  for (const m of members) {
    if (m.name.trim()) {
      idx.set(m.name.toLowerCase(), m.address)
      idx.set(m.name.split('#')[0].toLowerCase(), m.address)
    }
  }
  return idx
}

function resolveMention(t: Extract<Token, { type: 'mention' }>, index: Map<string, string>): string | undefined {
  return index.get(`${t.name}#${t.tag}`.toLowerCase()) ?? index.get(t.name.toLowerCase())
}

/** Does this message @-mention me (by resolved address or by my bare name)? */
export function mentionsMe(text: string, me: { address?: string; name?: string } | null, index: Map<string, string>): boolean {
  if (!me) return false
  const myName = me.name?.split('#')[0].toLowerCase()
  return parseMessage(text).some((t) => {
    if (t.type !== 'mention') return false
    const addr = resolveMention(t, index)
    return (addr && me.address && addr.toLowerCase() === me.address.toLowerCase()) || (!!myName && t.name.toLowerCase() === myName)
  })
}

export function MessageText({
  text,
  members,
  styles,
  onMention,
  onLocation
}: {
  text: string
  members: NearbyMember[]
  styles: { url: string; mention: string; location: string }
  /** A resolved @mention was clicked (address known). */
  onMention: (address: string, name: string, e: React.MouseEvent) => void
  /** A location link (x,y) was clicked. */
  onLocation: (x: number, y: number) => void
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
