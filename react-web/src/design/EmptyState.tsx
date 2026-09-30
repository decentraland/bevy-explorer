// EmptyState — centered icon/title/subtitle with optional CTA actions, for
// empty and error surfaces. Ported from dcl-react-ui.

import type { CSSProperties, ReactNode } from 'react'
import styles from './EmptyState.module.css'

export type EmptyStateActionVariant = 'solid' | 'outline' | 'ghost'

export interface EmptyStateAction {
  label: ReactNode
  onClick?: () => void
  href?: string
  variant?: EmptyStateActionVariant
  icon?: ReactNode
}

type EmptyStateVariant = 'inline' | 'screen' | 'card'

interface EmptyStateProps {
  icon?: ReactNode
  iconWash?: boolean
  title?: ReactNode
  subtitle?: ReactNode
  /** An array of action descriptors, or arbitrary action nodes. */
  actions?: EmptyStateAction[] | ReactNode
  variant?: EmptyStateVariant
  tone?: 'error'
  actionsGap?: number | string
  className?: string
  style?: CSSProperties
}

const CTA_VARIANT: Record<EmptyStateActionVariant, string> = {
  solid: '',
  outline: 'ctaOutline',
  ghost: 'ctaGhost'
}

function Action({ label, onClick, href, variant = 'solid', icon }: EmptyStateAction): React.JSX.Element {
  const cls = `${styles.cta} ${styles[CTA_VARIANT[variant]] ?? ''}`.trim()
  const inner = (
    <>
      {icon != null && (
        <span className={styles.ctaIcon} aria-hidden="true">
          {icon}
        </span>
      )}
      {label}
    </>
  )
  if (href != null) {
    return (
      <a className={cls} href={href} onClick={onClick}>
        {inner}
      </a>
    )
  }
  return (
    <button type="button" className={cls} onClick={onClick}>
      {inner}
    </button>
  )
}

const len = (v: number | string): string => (typeof v === 'number' ? `${v}px` : v)

const VARIANT_CLASS: Record<EmptyStateVariant, string> = {
  inline: 'inline',
  screen: 'screen',
  card: 'card'
}

export function EmptyState({
  icon,
  iconWash = false,
  title,
  subtitle,
  actions,
  variant,
  tone,
  actionsGap,
  className = '',
  style
}: EmptyStateProps): React.JSX.Element {
  const cls = [
    styles.es,
    variant ? styles[VARIANT_CLASS[variant]] : '',
    tone === 'error' ? styles.error : '',
    className
  ]
    .filter(Boolean)
    .join(' ')

  const gap = actionsGap != null ? actionsGap : tone === 'error' ? 40 : undefined
  const mergedStyle: CSSProperties | undefined =
    gap != null ? { ...style, ['--es-actions-gap' as string]: len(gap) } : style

  const isActionList = Array.isArray(actions)

  return (
    <div className={cls} style={mergedStyle}>
      {icon != null && (
        <div className={`${styles.icon} ${iconWash ? styles.iconWash : ''}`.trim()} aria-hidden="true">
          {icon}
        </div>
      )}

      {title != null && <p className={styles.title}>{title}</p>}
      {subtitle != null && <p className={styles.sub}>{subtitle}</p>}

      {isActionList
        ? actions.length > 0 && (
            <div className={styles.actions}>
              {actions.map((a, i) => (
                <Action key={i} {...a} />
              ))}
            </div>
          )
        : actions != null && <div className={styles.actions}>{actions}</div>}
    </div>
  )
}
