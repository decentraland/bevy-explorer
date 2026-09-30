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

import infoBase from '../assets/backpack/info-base.webp'
import infoCommon from '../assets/backpack/info-common.webp'
import infoUncommon from '../assets/backpack/info-uncommon.webp'
import infoRare from '../assets/backpack/info-rare.webp'
import infoEpic from '../assets/backpack/info-epic.webp'
import infoLegendary from '../assets/backpack/info-legendary.webp'
import infoMythic from '../assets/backpack/info-mythic.webp'
import infoUnique from '../assets/backpack/info-unique.webp'
import infoExotic from '../assets/backpack/info-exotic.webp'
import infoEmpty from '../assets/backpack/info-empty.webp'

const TILE: Record<string, string> = { base, common, uncommon, rare, epic, legendary, mythic, unique, exotic }
const PANEL: Record<string, string> = {
  base: infoBase, common: infoCommon, uncommon: infoUncommon, rare: infoRare, epic: infoEpic,
  legendary: infoLegendary, mythic: infoMythic, unique: infoUnique, exotic: infoExotic
}

/** Detail-panel background for a rarity (a 364px-wide 9-slice: 391px top, 200px bottom); no rarity = the empty panel. */
export function rarityPanel(rarity?: string | null): string {
  if (rarity == null) return infoEmpty
  return PANEL[rarity.toLowerCase()] ?? infoBase
}

export function rarityTile(rarity?: string): string {
  return TILE[(rarity ?? '').toLowerCase()] ?? base
}
