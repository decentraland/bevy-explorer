// Shared player-identity helpers (name color, #tag split, short address). Used by
// the chat, members, and friends UIs so the rarity-colored naming is consistent.

const ADDRESS_RE = /^0x[0-9a-fA-F]{6,}$/

// DCL rarity name colors — stable per seed (address) so each player keeps a color.
const RARITY = [
  '#73d3d3', '#acf8f8', '#ff8362', '#ff4bed', '#caff73', '#a14bf3',
  '#e8b9ff', '#fea217', '#81e1ff', '#ff7439', '#ffa25a', '#ffc95b',
  '#a0abff', '#c640cd'
]

// Wearable/emote rarity colors — Unity NftRarityColors.asset (source of truth).
export const RARITY_COLOR: Record<string, string> = {
  base: '#a09ba8',
  common: '#73d3d3',
  uncommon: '#ff8362',
  rare: '#34ce76',
  epic: '#438fff',
  legendary: '#b058ff',
  mythic: '#ff4bec',
  unique: '#ffc747',
  exotic: '#a5e242'
}

export function rarityColor(rarity?: string): string {
  return RARITY_COLOR[(rarity ?? 'base').toLowerCase()] ?? RARITY_COLOR.base
}

// Catalyst thumbnail for an emote URN — derived client-side so the wheel shows previews
// even if the scene relay didn't send a thumbnail. Strips a trailing `:tokenId` (on-chain NFTs).
const CATALYST_CONTENTS = 'https://peer.decentraland.org/lambdas/collections/contents'

// Direct catalyst thumbnail URL — used straight as an <img>/<image> src. The catalyst sends
// no Cross-Origin-Resource-Policy header, but the page runs COEP `credentialless`, under which
// a plain cross-origin <img> loads credentiallessly (no CORP needed) — so no proxy and no blob
// fetch is required (a fetch() WOULD be blocked; an <img> is not).
//
// Base emotes sometimes arrive as bare ids ("wave") from the engine's player data, but the
// catalyst contents endpoint 404s on those — it needs the full off-chain URN. Full URNs
// (urn:…) pass through; an owned-NFT urn carrying a trailing :tokenId is reduced to its item urn.
export function catalystThumbUrl(urn: string): string {
  const base = urn.startsWith('urn:') ? urn : `urn:decentraland:off-chain:base-emotes:${urn}`
  return `${CATALYST_CONTENTS}/${itemUrn(base)}/thumbnail`
}

// The contents endpoint serves the ITEM urn. A collections-v2 item urn ends with :<contract>:<itemId>
// (the itemId is required — stripping it 404s); an owned token urn appends a further :<tokenId>.
// Only drop that trailing tokenId — i.e. the last segment when the last TWO are both numeric.
function itemUrn(urn: string): string {
  const parts = urn.split(':')
  const n = parts.length
  if (n >= 2 && /^\d+$/.test(parts[n - 1]) && /^\d+$/.test(parts[n - 2])) return parts.slice(0, -1).join(':')
  return urn
}

function hash(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0
  return h
}

export function nameColor(seed: string): string {
  return RARITY[hash(seed) % RARITY.length]
}

export function shortAddr(s: string): string {
  return ADDRESS_RE.test(s) ? `${s.slice(0, 6)}…${s.slice(-4)}` : s
}

/** Split "Name#a1b2" into the colored base and a dimmer #tag. */
export function splitName(label: string): { base: string; tag: string } {
  const i = label.indexOf('#')
  return i >= 0 ? { base: label.slice(0, i), tag: label.slice(i) } : { base: label, tag: '' }
}
