// MaskIcon — a white-on-transparent image used as a CSS mask, so it takes `currentColor`.
import styles from './MaskIcon.module.css'

export function MaskIcon({ src, size = 24, className }: { src: string; size?: number; className?: string }): React.JSX.Element {
  return (
    <span
      className={`${styles.icon} ${className ?? ''}`.trim()}
      style={{ width: size, height: size, maskImage: `url(${src})`, WebkitMaskImage: `url(${src})` }}
      aria-hidden="true"
    />
  )
}
