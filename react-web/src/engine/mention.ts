// @mentions: how they're written and matched. Shared by the HUD and the bridge scene (pure).

/** A claimed name has no #suffix and isn't a raw address — the fallback when no profile says. */
export function looksClaimed(name: string): boolean {
  return name.trim().length > 0 && !name.includes('#') && !/^0x[0-9a-f]+$/i.test(name)
}

/** What follows "@": the name's letters and digits, plus "#" and the wallet's last 4 when the
 *  name isn't claimed (Name#1a2b). */
export function mentionName(name: string, address: string, claimed?: boolean): string {
  const base = name.split('#')[0].replace(/[^A-Za-z0-9]/g, '')
  return (claimed ?? looksClaimed(name)) || address.length < 4 ? base : `${base}#${address.slice(-4)}`
}

const MENTION_RE = /(?<=^|\s)@([A-Za-z0-9]{1,15}(?:#[A-Za-z0-9]{4})?)(?=\s|!|\?|\.|,|$)/g

/** Does the message @-mention exactly this mention name (case-insensitive)? */
export function mentionsName(message: string, mention: string): boolean {
  const want = mention.toLowerCase()
  for (const m of message.matchAll(MENTION_RE)) if (m[1].toLowerCase() === want) return true
  return false
}
