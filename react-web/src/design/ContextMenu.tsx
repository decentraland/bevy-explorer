// ContextMenu — a stacked menu surface (header, titles, separators, items,
// toggles) ported from the dcl-react-ui shared primitives.

import { useState, type ReactNode } from 'react'
import { Toggle } from './Toggle'
import styles from './ContextMenu.module.css'

export type ContextMenuItem =
  | { kind: 'separator' }
  | { kind: 'title'; label: ReactNode }
  | { kind: 'caption'; label: ReactNode; avatar?: ReactNode }
  | { kind: 'header'; name: ReactNode; tag?: ReactNode; address?: string; avatar?: ReactNode }
  | { kind: 'toggle'; label: ReactNode; icon?: ReactNode; checked?: boolean; onChange?: (checked: boolean) => void }
  | { kind?: 'item' | 'submenu'; label: ReactNode; icon?: ReactNode; danger?: boolean; onClick?: () => void }

interface ContextMenuProps {
  items: ContextMenuItem[]
}

function shortAddress(a: string): string {
  if (a.length <= 13) return a
  return `${a.slice(0, 6)}…${a.slice(-4)}`
}

export function ContextMenu({ items }: ContextMenuProps): React.JSX.Element {
  return (
    <div className={styles.ctx} role="menu">
      {items.map((it, i) => {
        if (it.kind === 'separator') return <div className={styles.sep} key={i} />

        if (it.kind === 'title') {
          return (
            <div className={styles.title} key={i}>
              {it.label}
            </div>
          )
        }

        if (it.kind === 'caption') {
          return (
            <div className={styles.caption} key={i}>
              {it.avatar != null && it.avatar}
              <span className={styles.captionLabel}>{it.label}</span>
            </div>
          )
        }

        if (it.kind === 'header') {
          return (
            <div className={styles.header} key={i}>
              {it.avatar != null && it.avatar}
              <div className={styles.hinfo}>
                <div className={styles.hname}>
                  {it.name}
                  {it.tag != null && <span className={styles.htag}>{it.tag}</span>}
                </div>
                {it.address != null && <span className={styles.wallet}>{shortAddress(it.address)}</span>}
              </div>
            </div>
          )
        }

        if (it.kind === 'toggle') return <ToggleItem item={it} key={i} />

        return (
          <button
            type="button"
            className={`${styles.item} ${it.danger ? styles.itemDanger : ''}`.trim()}
            role="menuitem"
            key={i}
            onClick={it.onClick}
          >
            {it.icon != null && <span className={styles.icon}>{it.icon}</span>}
            <span className={styles.label}>{it.label}</span>
            {it.kind === 'submenu' && <span className={styles.chev}>›</span>}
          </button>
        )
      })}
    </div>
  )
}

function ToggleItem({ item }: { item: Extract<ContextMenuItem, { kind: 'toggle' }> }): React.JSX.Element {
  const [on, setOn] = useState(!!item.checked)
  const change = (next: boolean): void => {
    setOn(next)
    item.onChange?.(next)
  }
  return (
    <label className={`${styles.item} ${styles.itemToggle}`} role="menuitemcheckbox" aria-checked={on}>
      {item.icon != null && <span className={styles.icon}>{item.icon}</span>}
      <span className={styles.label}>{item.label}</span>
      <Toggle checked={on} onChange={change} />
    </label>
  )
}
