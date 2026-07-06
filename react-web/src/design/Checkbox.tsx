// Checkbox — square box with an orange tick. Supports controlled (`checked`)
// or uncontrolled (`defaultChecked`) use; design references eordano/dcl-react-ui.

import { useState } from 'react'
import styles from './Checkbox.module.css'

interface CheckboxProps {
  checked?: boolean
  defaultChecked?: boolean
  onChange?: (checked: boolean) => void
  children?: React.ReactNode
}

export function Checkbox({
  checked,
  defaultChecked = false,
  onChange,
  children
}: CheckboxProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultChecked)
  const isControlled = checked !== undefined
  const on = isControlled ? checked : internal

  const toggle = (): void => {
    if (!isControlled) setInternal(!on)
    onChange?.(!on)
  }

  return (
    <label className={styles.checkbox}>
      <input
        type="checkbox"
        className={styles.input}
        checked={on}
        onChange={toggle}
      />
      <span className={`${styles.box} ${on ? styles.checked : ''}`.trim()}>
        {on && (
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path
              d="M3 8.5l3 3 7-7"
              fill="none"
              stroke="var(--accent)"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
      </span>
      <span className={styles.label}>{children}</span>
    </label>
  )
}
