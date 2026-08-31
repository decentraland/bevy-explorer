// PageHeader — a page/section heading with optional subtitle, leading slot, and
// trailing actions. Ported from dcl-react-ui.

import type { ElementType, ReactNode } from 'react'
import styles from './PageHeader.module.css'

type PageHeaderSize = 'default' | 'hero' | 'section'

export interface PageHeaderProps {
  title: ReactNode
  subtitle?: ReactNode
  actions?: ReactNode
  leading?: ReactNode
  size?: PageHeaderSize
  /** Override the title element (defaults to h1, or h2 for size="section"). */
  as?: ElementType
  className?: string
}

const SIZE_CLASS: Record<PageHeaderSize, string> = {
  default: 'default',
  hero: 'hero',
  section: 'section'
}

export function PageHeader({
  title,
  subtitle,
  actions,
  leading,
  size = 'default',
  as,
  className = ''
}: PageHeaderProps): React.JSX.Element {
  const Title: ElementType = as ?? (size === 'section' ? 'h2' : 'h1')
  const cls = `${styles.pageheader} ${styles[SIZE_CLASS[size]]} ${className}`.trim()
  return (
    <div className={cls}>
      <div className={styles.main}>
        {leading != null && <div className={styles.leading}>{leading}</div>}
        <div className={styles.titles}>
          <Title className={styles.title}>{title}</Title>
          {subtitle != null && <p className={styles.subtitle}>{subtitle}</p>}
        </div>
      </div>
      {actions != null && <div className={styles.actions}>{actions}</div>}
    </div>
  )
}
