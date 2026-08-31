// Toggle (switch) — DCL ruby pill with a white knob (matches the Unity settings
// "Fullscreen" control). Used by Settings and anywhere a boolean control is needed.

import styles from './Toggle.module.css'

interface ToggleProps {
  checked: boolean
  onChange: (checked: boolean) => void
  disabled?: boolean
  'aria-label'?: string
}

export function Toggle({
  checked,
  onChange,
  disabled = false,
  'aria-label': ariaLabel
}: ToggleProps): React.JSX.Element {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      className={`${styles.track} ${checked ? styles.on : ''}`.trim()}
      onClick={() => onChange(!checked)}
    >
      <span className={styles.knob} />
    </button>
  )
}
