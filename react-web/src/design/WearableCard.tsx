// WearableCard — a backpack item tile: the rarity texture with the thumbnail, a rarity-tinted
// corner flap with the category glyph, an orange ring when equipped, and on hover a scale-up with a
// glow panel holding the EQUIP / UNEQUIP button under the tile.

import { useState } from 'react'
import { rarityTile } from './rarityArt'
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
  /** Open in the detail panel (no visual ring; exposed as aria-pressed). */
  selected?: boolean
  isNew?: boolean
  /** Shows the SMART badge. */
  isSmart?: boolean
  incompatible?: boolean
  /** False for a required category (body shape, eyes…): equipped, it offers no UNEQUIP. */
  unequippable?: boolean
  /** Body-part glyph shown in the top-left flap. */
  categoryIcon?: React.ReactNode
  /** Emote wheel slot the item is equipped in, shown top-right (emote cards). */
  slotNumber?: number
  /** Card click — selects the item (shows its detail; does not equip or preview). */
  onClick?: () => void
  /** Card double-click — the explicit equip action (persists), same as the button. */
  onDoubleClick?: () => void
  /** Hover EQUIP/UNEQUIP click — the explicit equip action (persists). */
  onEquip?: () => void
}

export function WearableCard({
  thumbnail,
  name,
  rarity = 'base',
  equipped = false,
  selected = false,
  isNew = false,
  isSmart = false,
  incompatible = false,
  unequippable = true,
  categoryIcon,
  slotNumber,
  onClick,
  onDoubleClick,
  onEquip
}: WearableCardProps): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const action = incompatible && !equipped ? null : equipped ? (unequippable ? 'UNEQUIP' : null) : 'EQUIP'
  return (
    <button
      type="button"
      className={`${styles.card} ${equipped ? styles.equipped : ''} ${incompatible ? styles.incompatible : ''}`.trim()}
      data-rarity={rarity}
      title={name}
      aria-label={name}
      aria-pressed={selected}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
    >
      <span className={styles.hoverPanel} aria-hidden="true" />
      <span className={styles.body} style={{ backgroundImage: `url(${rarityTile(rarity)})` }}>
        {thumbnail && !failed && <img className={styles.thumb} src={thumbnail} alt="" onError={() => setFailed(true)} />}
        {categoryIcon != null && (
          <span className={styles.flap}>
            <span className={styles.flapTint} aria-hidden="true" />
            <span className={styles.flapIcon}>{categoryIcon}</span>
          </span>
        )}
        {isNew && <span className={styles.new}>NEW</span>}
        {isSmart && <span className={styles.smart}>SMART</span>}
        {slotNumber != null && <span className={styles.slotNumber}>{slotNumber}</span>}
        {incompatible && (
          <span className={styles.incompatibleCover}>
            <span className={styles.incompatibleNote}>Incompatible with body shape</span>
          </span>
        )}
      </span>
      <span className={styles.ring} aria-hidden="true" />
      {action != null && (
        <span
          className={`${styles.action} ${equipped ? styles.unequip : styles.equip}`}
          role="button"
          onClick={(e) => { e.stopPropagation(); onEquip?.() }}
        >
          {action}
        </span>
      )}
    </button>
  )
}
