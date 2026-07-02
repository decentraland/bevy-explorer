// ControlButton — the small icon control used across the HUD (close, back, menu,
// emoji toggle, emoji tabs, count pills…). One token-driven primitive so every
// such button is consistent. Heavier rail buttons (tooltip + badge) compose this
// via IconButton.

import styles from './ControlButton.module.css'

type Variant = 'ghost' | 'solid'
type Shape = 'square' | 'circle' | 'pill'
type Size = 'sm' | 'md'

interface ControlButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** ghost = transparent→white-10 hover (default); solid = dark fill. */
  variant?: Variant
  /** square (default), circle, or pill (for label + value like a count). */
  shape?: Shape
  /** md = 30px (default), sm = 26px. */
  size?: Size
  /** Selected/pressed state. */
  active?: boolean
}

export function ControlButton({
  variant = 'ghost',
  shape = 'square',
  size = 'md',
  active = false,
  className = '',
  type = 'button',
  ...rest
}: ControlButtonProps): React.JSX.Element {
  return (
    <button
      type={type}
      className={`${styles.btn} ${styles[variant]} ${styles[shape]} ${styles[size]} ${
        active ? styles.active : ''
      } ${className}`.trim()}
      aria-pressed={active}
      {...rest}
    />
  )
}
