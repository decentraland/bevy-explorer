// EquippedItemCard — a read-only equipped-item tile (passport "Equipped Wearables/Emotes",
// matching unity-explorer's EquippedItem_PassportFieldView / bevy-ui-scene's
// PassportEquippedItem): a dark card whose SQUARE top area carries the rarity-gradient
// background + thumbnail + a rarity-colored corner flap with the category glyph
// (bevy-ui-scene's `rarity-background-*` / `rarity-corner-*` / `category-*` atlas sprites,
// recreated in CSS), then name (ellipsized) + rarity tag below on the dark body. When the item
// has a marketplace listing, hovering pops the card with an animated gradient border ring +
// glow (Unity's hover frame) and extends it downward to reveal a SHOP button inside; base/
// off-chain items (no listing) get neither. Distinct from WearableCard (the Backpack's
// clickable, equip-focused grid tile, which has no caption) — this one is purely for display.

import { useState } from 'react'
import { Button } from './Button'
import styles from './EquippedItemCard.module.css'

export interface EquippedItemCardProps {
  thumbnail?: string
  name?: string
  rarity?: string
  shopUrl?: string
  /** Body-part / item-kind glyph shown in the top-left rarity-colored corner flap. */
  categoryIcon?: React.ReactNode
}

export function EquippedItemCard({ thumbnail, name, rarity, shopUrl, categoryIcon }: EquippedItemCardProps): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  return (
    <div className={`${styles.card} ${shopUrl != null ? styles.hasShop : ''}`.trim()} data-rarity={rarity ?? 'base'}>
      <div className={styles.thumbWrap}>
        {thumbnail && !failed ? (
          <img className={styles.thumb} src={thumbnail} alt="" onError={() => setFailed(true)} />
        ) : (
          <span className={styles.placeholder} />
        )}
        {categoryIcon != null && <span className={styles.corner}>{categoryIcon}</span>}
      </div>
      <span className={styles.name} title={name}>{name}</span>
      {/* Always rendered ('base' fallback) so every tile has the same height. */}
      <span className={styles.rarityTag}>{rarity ?? 'base'}</span>
      {shopUrl != null && (
        <Button href={shopUrl} target="_blank" rel="noopener" size="sm" className={styles.shopBtn}>
          Shop
        </Button>
      )}
    </div>
  )
}
