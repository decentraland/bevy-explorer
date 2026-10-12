// Pager — prev / next arrows around a sliding window of up to five page numbers. The arrows wrap
// (prev on the first page goes to the last). Pages are 0-based; hidden when there's one page.
import { MaskIcon } from './MaskIcon'
import arrowLeftIcon from '../assets/backpack/icon-arrow-left.webp'
import styles from './Pager.module.css'

const WINDOW = 5

// The current page stays centred once past the first half, and the window clamps at both ends.
export function pageWindow(current: number, count: number): number[] {
  const size = Math.min(WINDOW, count)
  const half = Math.floor(WINDOW / 2)
  const start = current > half ? Math.min(current - half, count - size) : 0
  return Array.from({ length: size }, (_, i) => start + i)
}

export function Pager({ page, count, onChange, className }: { page: number; count: number; onChange: (page: number) => void; className?: string }): React.JSX.Element | null {
  if (count <= 1) return null
  return (
    <nav className={`${styles.pager} ${className ?? ''}`.trim()} aria-label="Pages">
      <button type="button" className={styles.arrow} aria-label="Previous page" onClick={() => onChange(page === 0 ? count - 1 : page - 1)}>
        <MaskIcon src={arrowLeftIcon} size={12} />
      </button>
      <span className={styles.pages}>
        {pageWindow(page, count).map((p) => (
          <button
            key={p}
            type="button"
            className={`${styles.page} ${p === page ? styles.current : ''}`.trim()}
            aria-current={p === page ? 'page' : undefined}
            onClick={() => onChange(p)}
          >
            {p + 1}
          </button>
        ))}
      </span>
      <button type="button" className={`${styles.arrow} ${styles.next}`} aria-label="Next page" onClick={() => onChange(page >= count - 1 ? 0 : page + 1)}>
        <MaskIcon src={arrowLeftIcon} size={12} />
      </button>
    </nav>
  )
}
