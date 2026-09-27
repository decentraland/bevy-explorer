// Chip — a breadcrumb filter chip: orange when selected, off-white otherwise, with an optional
// icon and an optional clear (✕) box.
import { MaskIcon } from './MaskIcon'
import closeIcon from '../assets/backpack/icon-close.webp'
import styles from './Chip.module.css'

export function Chip({
  label,
  icon,
  selected = false,
  onClick,
  onClear,
  clearLabel = 'Clear'
}: {
  label: string
  icon?: React.ReactNode
  selected?: boolean
  onClick?: () => void
  onClear?: () => void
  clearLabel?: string
}): React.JSX.Element {
  return (
    <span className={`${styles.chip} ${selected ? styles.selected : ''}`.trim()}>
      <button type="button" className={styles.main} aria-pressed={selected} onClick={onClick}>
        {icon}
        <span className={styles.label}>{label}</span>
      </button>
      {onClear != null && (
        <button type="button" className={styles.clear} aria-label={clearLabel} onClick={onClear}>
          <MaskIcon src={closeIcon} size={10} />
        </button>
      )}
    </span>
  )
}
