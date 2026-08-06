// EquippedItemCard — a read-only equipped-item tile (passport "Equipped Wearables/Emotes"):
// thumbnail on a rarity-gradient background, name + rarity tag below. When the item has a
// marketplace listing, hovering reveals a SHOP button beneath the card with a matching border
// glow; base/off-chain items (no listing) get neither. Distinct from WearableCard (the Backpack's
// clickable, equip-focused grid tile, which has no caption) — this one is purely for display.

import { useState } from 'react'
import { Button } from './Button'
import { rarityColor } from '../lib/identity'
import styles from './EquippedItemCard.module.css'

export interface EquippedItemCardProps {
  thumbnail?: string
  name?: string
  rarity?: string
  shopUrl?: string
}

export function EquippedItemCard({ thumbnail, name, rarity, shopUrl }: EquippedItemCardProps): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const color = rarityColor(rarity)
  return (
    <div
      className={`${styles.card} ${shopUrl != null ? styles.hasShop : ''}`.trim()}
      style={{ '--rm': color } as React.CSSProperties}
    >
      <div className={styles.thumbWrap}>
        {thumbnail && !failed ? (
          <img className={styles.thumb} src={thumbnail} alt="" onError={() => setFailed(true)} />
        ) : (
          <span className={styles.placeholder} />
        )}
      </div>
      <span className={styles.name} title={name}>{name}</span>
      {rarity != null && (
        <span className={styles.rarityTag} style={{ background: color }}>{rarity}</span>
      )}
      {shopUrl != null && (
        <Button href={shopUrl} target="_blank" rel="noopener noreferrer" size="sm" className={styles.shopBtn}>
          Shop
        </Button>
      )}
    </div>
  )
}
