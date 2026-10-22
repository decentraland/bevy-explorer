// TextInput — the plain single-line field. `SearchField` covers search (pill + magnifier);
// this is everything else: names, link titles, dates. Controlled or uncontrolled, like the
// other input primitives here.

import { useState } from 'react'
import { Icon, type IconName } from './icons'
import styles from './TextInput.module.css'

export interface TextInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value' | 'size'> {
  value?: string
  defaultValue?: string
  onChange?: (value: string) => void
  /** Light = the white field used on solid panels (Settings, the passport's edit mode). */
  variant?: 'dark' | 'light'
  /** Red border + `aria-invalid`, for a value the user still has to fix. */
  invalid?: boolean
  /** An icon action inside the field's right edge (e.g. a dice that picks another value). */
  action?: { icon: IconName; label: string; onClick: () => void }
}

export function TextInput({
  value,
  defaultValue = '',
  onChange,
  variant = 'dark',
  invalid = false,
  action,
  className,
  ...rest
}: TextInputProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultValue)
  const isControlled = value !== undefined
  const v = isControlled ? value : internal

  const input = (
    <input
      {...rest}
      className={[styles.input, styles[variant], invalid ? styles.invalid : '', action ? styles.withAction : '', className ?? ''].filter(Boolean).join(' ')}
      value={v}
      aria-invalid={invalid || undefined}
      onChange={(e) => {
        if (!isControlled) setInternal(e.target.value)
        onChange?.(e.target.value)
      }}
    />
  )
  if (action == null) return input
  return (
    <div className={styles.wrap}>
      {input}
      <button type="button" className={styles.action} aria-label={action.label} title={action.label} onClick={action.onClick} disabled={rest.disabled}>
        <Icon name={action.icon} size={18} />
      </button>
    </div>
  )
}
