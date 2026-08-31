// Panel — the shared dark surface for HUD widgets and menus.

import styles from './Panel.module.css'

interface PanelProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Frosted translucent surface (for in-world overlays). */
  blur?: boolean
}

export function Panel({
  blur = false,
  className = '',
  ...rest
}: PanelProps): React.JSX.Element {
  return (
    <div
      className={`${styles.panel} ${blur ? styles.blur : ''} ${className}`.trim()}
      {...rest}
    />
  )
}
