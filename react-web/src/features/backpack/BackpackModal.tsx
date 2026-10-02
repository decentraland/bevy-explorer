// The Backpack as a modal over the lobby: the page's content in a centred frame over a dimmed
// lobby, with its own close button and no menu bar, so the lobby is still where you return to.

import { Close, HeaderButton } from '../../design'
import styles from './BackpackModal.module.css'

export function BackpackModal({ onClose, children }: { onClose: () => void; children: React.ReactNode }): React.JSX.Element {
  return (
    <div className={styles.root}>
      <div className={styles.dim} onClick={onClose} aria-hidden="true" />
      <div className={styles.frame} role="dialog" aria-label="Backpack">
        {children}
        <HeaderButton className={styles.close} aria-label="Close" onClick={onClose}>
          <Close size={12} />
        </HeaderButton>
      </div>
    </div>
  )
}
