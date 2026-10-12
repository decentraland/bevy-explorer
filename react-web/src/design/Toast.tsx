// Toast — a transient notice at the top of the screen (friend blocked, "X is online", a failed
// action). Imperative like the popups: showToast() from anywhere, <ToastHost/> mounted once. Unlike a
// popup it holds no input focus and never counts as an open popup.

import { useSyncExternalStore, type ReactNode } from 'react'
import styles from './Toast.module.css'

export interface ToastOptions {
  /** Leading visual, e.g. an Avatar. */
  icon?: ReactNode
  tone?: 'default' | 'error'
  /** ms before it goes away (default 5000). */
  duration?: number
}

interface ToastItem extends ToastOptions {
  id: number
  content: ReactNode
}

let items: ToastItem[] = []
let nextId = 0
const listeners = new Set<() => void>()
const emit = (): void => listeners.forEach((l) => l())

export function showToast(content: ReactNode, options: ToastOptions = {}): void {
  const id = ++nextId
  items = [...items, { id, content, ...options }]
  emit()
  setTimeout(() => {
    items = items.filter((t) => t.id !== id)
    emit()
  }, options.duration ?? 5000)
}

const subscribe = (cb: () => void): (() => void) => {
  listeners.add(cb)
  return () => listeners.delete(cb)
}
const getSnapshot = (): ToastItem[] => items

export function ToastHost(): React.JSX.Element {
  const list = useSyncExternalStore(subscribe, getSnapshot)
  return (
    <div className={styles.host} aria-live="polite">
      {list.map((t) => (
        <div key={t.id} className={`${styles.toast} ${t.tone === 'error' ? styles.error : ''}`.trim()} role="status">
          {t.icon}
          <span className={styles.text}>{t.content}</span>
        </div>
      ))}
    </div>
  )
}
