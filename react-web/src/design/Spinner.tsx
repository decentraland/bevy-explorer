// Spinner — circular loading indicator (ruby arc on a faint track).
// Design references eordano/dcl-react-ui.

import styles from './Spinner.module.css'

interface SpinnerProps {
  size?: number
  color?: string
}

export function Spinner({ size = 28, color }: SpinnerProps): React.JSX.Element {
  const style = {
    '--sz': `${size}px`,
    ...(color ? { '--spinner-arc': color } : {})
  } as React.CSSProperties
  return (
    <span className={styles.spinner} style={style} role="status" aria-label="Loading">
      <svg viewBox="0 0 50 50" width={size} height={size}>
        <circle className={styles.track} cx="25" cy="25" r="20" fill="none" strokeWidth="5" />
        <circle
          className={styles.arc}
          cx="25"
          cy="25"
          r="20"
          fill="none"
          strokeWidth="5"
          strokeLinecap="round"
          strokeDasharray="90 160"
        />
      </svg>
    </span>
  )
}
