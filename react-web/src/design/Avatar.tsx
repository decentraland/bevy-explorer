// Avatar — circular profile picture with a colored-initial fallback and optional
// status dot. Shared across chat bubbles, member rows, friends, and the profile.

import { useState } from 'react'
import styles from './Avatar.module.css'

type Status = 'online' | 'away' | 'offline'

interface AvatarProps {
  /** Face snapshot URL; falls back to the colored initial when absent or it fails. */
  src?: string
  /** Used for the initial fallback + alt text. */
  name: string
  /** Fallback background / ring tint. */
  color?: string
  /** Diameter in px (default 32). */
  size?: number
  status?: Status
  /** The picture inset in its colour disc under a thin ring, as on the sidebar rail. */
  framed?: boolean
  /** Status dot corner: bottom-right (default) or a smaller dot at the top-right (friends list). */
  dotPosition?: 'bottom' | 'top'
  className?: string
}

function initialOf(name: string): string {
  const m = name.replace(/^[@#]/, '').match(/[a-z0-9]/i)
  return (m ? m[0] : name[0] || '?').toUpperCase()
}

export function Avatar({
  src,
  name,
  color = 'var(--fill-4)',
  size = 32,
  status,
  framed = false,
  dotPosition = 'bottom',
  className = ''
}: AvatarProps): React.JSX.Element {
  const [failed, setFailed] = useState(false)
  const showImg = src && !failed
  const dot = Math.max(6, Math.round(size * (dotPosition === 'top' ? 0.15 : 0.18)))
  return (
    <span
      className={`${styles.root} ${framed ? styles.framed : ''} ${className}`.trim()}
      style={{ width: size, height: size, background: color, fontSize: Math.round(size * 0.42) }}
    >
      {showImg ? (
        <img className={styles.img} src={src} alt={name} onError={() => setFailed(true)} />
      ) : (
        initialOf(name)
      )}
      {status && (
        <span
          className={`${styles.dot} ${styles[status]} ${dotPosition === 'top' ? styles.dotTop : ''}`.trim()}
          style={{ width: dot, height: dot }}
        />
      )}
    </span>
  )
}
