// WearableCard — a backpack item tile (matches the Figma NFT Items "Backpack items"
// + button variants): rarity-gradient background, thumbnail, equipped highlight, and
// NEW / ×count badges. Used by the Backpack catalog.

import { useState } from 'react'
import styles from './WearableCard.module.css'

export type Rarity =
  | 'base'
  | 'common'
  | 'uncommon'
  | 'rare'
  | 'epic'
  | 'legendary'
  | 'mythic'
  | 'unique'
  | 'exotic'

interface WearableCardProps {
  thumbnail?: string
  name?: string
  rarity?: Rarity
  equipped?: boolean
  selected?: boolean
  isNew?: boolean
  count?: number
  incompatible?: boolean
  /** Body-part glyph shown in the top-left flap (matches Unity's category badge). */
  categoryIcon?: React.ReactNode
  /** Card click — selects the item (previews, does not persist). */
  onClick?: () => void
  /** Hover EQUIP/UNEQUIP pill click — the explicit equip action (persists). */
  onEquip?: () => void
}

export function WearableCard({
  thumbnail,
  name,
  rarity = 'base',
  equipped = false,
  selected = false,
  isNew = false,
  count,
  incompatible = false,
  categoryIcon,
  onClick,
  onEquip
}: WearableCardProps): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  return (
    <button
      type="button"
      className={`${styles.card} ${styles[rarity]} ${equipped ? styles.equipped : ''} ${selected ? styles.selected : ''} ${incompatible ? styles.incompatible : ''}`.trim()}
      title={name}
      aria-label={name}
      onClick={onClick}
    >
      {categoryIcon != null && <span className={styles.flap}>{categoryIcon}</span>}
      {thumbnail && !failed ? (
        <img className={styles.thumb} src={thumbnail} alt="" onError={() => setFailed(true)} />
      ) : (
        <span className={styles.placeholder} />
      )}
      {equipped && <span className={styles.equippedDot} aria-hidden="true" />}
      {isNew && <span className={styles.new}>NEW</span>}
      {count != null && count > 1 && <span className={styles.count}>×{count}</span>}
      <span
        className={`${styles.action} ${equipped ? styles.unequip : styles.equip}`}
        role="button"
        onClick={(e) => { e.stopPropagation(); onEquip?.() }}
      >
        {equipped ? 'UNEQUIP' : 'EQUIP'}
      </span>
    </button>
  )
}
