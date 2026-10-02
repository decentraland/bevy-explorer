// The Backpack as a modal over the lobby: the page's content in a centred frame over a dimmed
// lobby, with its own close button and no menu bar, so the lobby is still where you return to.

import { useRef } from 'react'
import { Close, HeaderButton } from '../../design'
import { useSession } from '../session/SessionContext'
import { holeMask } from '../../lib/holeMask'
import styles from './BackpackModal.module.css'

export function BackpackModal({ onClose, children }: { onClose: () => void; children: React.ReactNode }): React.JSX.Element {
  const frameRef = useRef<HTMLDivElement>(null)
  const preview = useSession().avatarPreviewRect
  // the dim must not cover the engine's avatar, which shows through the frame's hole
  const frame = frameRef.current?.getBoundingClientRect()
  const hole = preview != null && frame != null ? clip(preview, frame) : null
  return (
    <div className={styles.root}>
      <div className={styles.dim} style={hole != null ? holeMask(hole) : undefined} onClick={onClose} aria-hidden="true" />
      <div ref={frameRef} className={styles.frame} role="dialog" aria-label="Backpack">
        {children}
        <HeaderButton className={styles.close} aria-label="Close" onClick={onClose}>
          <Close size={12} />
        </HeaderButton>
      </div>
    </div>
  )
}

type Rect = { x: number; y: number; width: number; height: number }

// The layers under the modal open their hole a little wider than the page's own, so its
// anti-aliased edge blends into the engine's backdrop rather than the dark layers beneath.
const HOLE_BLEED = 2

/** The part of `rect` inside the modal frame (both in screen CSS px), widened by the bleed, or null. */
export function clip(rect: Rect, frame: DOMRect): Rect | null {
  const x = Math.max(rect.x - HOLE_BLEED, frame.left)
  const y = Math.max(rect.y - HOLE_BLEED, frame.top)
  const width = Math.min(rect.x + rect.width + HOLE_BLEED, frame.right) - x
  const height = Math.min(rect.y + rect.height + HOLE_BLEED, frame.bottom) - y
  return width > 0 && height > 0 ? { x, y, width, height } : null
}
