// Rail — a horizontal carousel paged a whole page at a time: dots below (one per page, none for a
// single page) and prev/next arrows that fade in while the section is hovered.

import { Children, useCallback, useEffect, useRef, useState } from 'react'
import arrowLeft from '../assets/lobby/arrow-left.png'
import arrowRight from '../assets/lobby/arrow-right.png'
import { MaskIcon } from './MaskIcon'
import styles from './Rail.module.css'

export function Rail({
  perPage,
  gap,
  children,
  className
}: {
  perPage: number
  /** Space between cards, px. */
  gap: number
  children: React.ReactNode
  className?: string
}): React.JSX.Element {
  const viewport = useRef<HTMLDivElement>(null)
  const count = Children.count(children)
  const pages = Math.max(1, Math.ceil(count / perPage))
  const [page, setPage] = useState(0)

  const goTo = useCallback(
    (p: number) => {
      const el = viewport.current
      const next = Math.max(0, Math.min(pages - 1, p))
      if (el == null) return
      const card = el.firstElementChild?.firstElementChild as HTMLElement | null
      const stride = (card?.offsetWidth ?? 0) + gap
      el.scrollTo({ left: Math.min(next * perPage * stride, el.scrollWidth - el.clientWidth), behavior: 'smooth' })
    },
    [gap, pages, perPage]
  )

  useEffect(() => {
    const el = viewport.current
    if (el == null) return
    const onScroll = (): void => {
      const max = el.scrollWidth - el.clientWidth
      setPage(max <= 0 ? 0 : Math.round((el.scrollLeft / max) * (pages - 1)))
    }
    el.addEventListener('scroll', onScroll, { passive: true })
    return () => el.removeEventListener('scroll', onScroll)
  }, [pages])

  useEffect(() => {
    viewport.current?.scrollTo({ left: 0 })
    setPage(0)
  }, [count])

  return (
    <div className={`${styles.rail} ${className ?? ''}`.trim()}>
      <div ref={viewport} className={styles.viewport}>
        <div className={styles.track} style={{ gap }}>
          {children}
        </div>
      </div>
      {pages > 1 && (
        <>
          <div className={styles.arrows}>
            <button type="button" className={styles.arrow} aria-label="Previous" hidden={page === 0} onClick={() => goTo(page - 1)}>
              <MaskIcon src={arrowLeft} size={15} className={styles.arrowIcon} />
            </button>
            <button type="button" className={`${styles.arrow} ${styles.next}`} aria-label="Next" hidden={page === pages - 1} onClick={() => goTo(page + 1)}>
              <MaskIcon src={arrowRight} size={15} className={styles.arrowIcon} />
            </button>
          </div>
          <div className={styles.dots} role="tablist">
            {Array.from({ length: pages }, (_, i) => (
              <button
                key={i}
                type="button"
                role="tab"
                aria-selected={i === page}
                aria-label={`Page ${i + 1}`}
                className={`${styles.dot} ${i === page ? styles.dotActive : ''}`.trim()}
                onClick={() => goTo(i)}
              />
            ))}
          </div>
        </>
      )}
    </div>
  )
}
