// EquippedItemCard — a read-only equipped-item tile (passport "Equipped Wearables/Emotes",
// matching unity-explorer's EquippedItem_PassportFieldView): full-bleed thumbnail on a
// rarity-gradient background, category flap top-left, name + rarity tag pinned to the bottom.
// When the item has a marketplace listing, hovering pops the card and extends its border
// downward to reveal a SHOP button inside it; base/off-chain items (no listing) get neither.
// Distinct from WearableCard (the Backpack's clickable, equip-focused grid tile, which has no
// caption) — this one is purely for display.

import { useState } from 'react'
import { Button } from './Button'
import { rarityColor } from '../lib/identity'
import styles from './EquippedItemCard.module.css'

export interface EquippedItemCardProps {
  thumbnail?: string
  name?: string
  rarity?: string
  shopUrl?: string
  /** Body-part / item-kind glyph shown in the top-left flap (matches Unity's category badge). */
  categoryIcon?: React.ReactNode
}

export function EquippedItemCard({ thumbnail, name, rarity, shopUrl, categoryIcon }: EquippedItemCardProps): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const color = rarityColor(rarity)
  return (
    <div
      className={`${styles.card} ${shopUrl != null ? styles.hasShop : ''}`.trim()}
      style={{ '--rm': color } as React.CSSProperties}
    >
      {categoryIcon != null && <span className={styles.flap}>{categoryIcon}</span>}
      <div className={styles.thumbWrap}>
        {thumbnail && !failed ? (
          <img className={styles.thumb} src={thumbnail} alt="" onError={() => setFailed(true)} />
        ) : (
          <span className={styles.placeholder} />
        )}
      </div>
      <span className={styles.name} title={name}>{name}</span>
      {rarity != null && <span className={styles.rarityTag}>{rarity}</span>}
      {shopUrl != null && (
        <Button href={shopUrl} target="_blank" rel="noopener" size="sm" className={styles.shopBtn}>
          Shop
        </Button>
      )}
    </div>
  )
}
