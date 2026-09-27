// Rarity background textures for item tiles (cards, slots), keyed by rarity; base is the fallback.
import base from '../assets/backpack/rarity-base.webp'
import common from '../assets/backpack/rarity-common.webp'
import uncommon from '../assets/backpack/rarity-uncommon.webp'
import rare from '../assets/backpack/rarity-rare.webp'
import epic from '../assets/backpack/rarity-epic.webp'
import legendary from '../assets/backpack/rarity-legendary.webp'
import mythic from '../assets/backpack/rarity-mythic.webp'
import unique from '../assets/backpack/rarity-unique.webp'
import exotic from '../assets/backpack/rarity-exotic.webp'

const TILE: Record<string, string> = { base, common, uncommon, rare, epic, legendary, mythic, unique, exotic }

export function rarityTile(rarity?: string): string {
  return TILE[(rarity ?? '').toLowerCase()] ?? base
}
